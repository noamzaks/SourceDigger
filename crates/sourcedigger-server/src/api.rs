use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use git2::Repository;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sourcedigger_index::Kind;
use sourcedigger_index::diff::Action;
use sourcedigger_index::git::remote_versions;

use crate::db::Database;
use crate::digger::Digger;
use crate::digs::{Dig, DigScope};
use crate::history::{History, Revision};
use crate::query::NamePattern;
use crate::sources::{Source, SourceSettings, SourceSummary};
use crate::symbols::{EventFilter, EventPage, EventRecord, VersionRecord};

#[derive(Clone)]
pub struct AppState {
    pub database_path: PathBuf,
    pub repositories: PathBuf,
    pub wake: Sender<()>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api", get(index))
        .route("/api/sources", get(list_sources).post(create_source))
        .route(
            "/api/sources/{source}",
            get(get_source).put(update_source).delete(delete_source),
        )
        .route("/api/sources/{source}/names", get(names))
        .route("/api/sources/{source}/events", get(events))
        .route("/api/sources/{source}/versions", get(list_versions))
        .route("/api/sources/{source}/symbols/{symbol}", get(symbol))
        .route("/api/sources/{source}/symbols/{symbol}/code", get(code))
        .route(
            "/api/sources/{source}/symbols/{symbol}/revisions",
            get(revisions),
        )
        .route("/api/sources/{source}/digs", get(list_digs).post(start_dig))
        .route("/api/digs/{dig}", get(get_dig))
        .route("/api/preview", post(preview))
        .with_state(state)
}

async fn index() -> Json<serde_json::Value> {
    const ENDPOINTS: &[(&str, &str, &str)] = &[
        (
            "GET",
            "/api/sources",
            "All sources with their version range and last dig",
        ),
        (
            "POST",
            "/api/sources",
            "Add a source; body: {name, url, tag_pattern, file_pattern, viewer_url, refresh_hours}",
        ),
        (
            "GET",
            "/api/sources/{source}",
            "One source with its version range and last dig",
        ),
        (
            "PUT",
            "/api/sources/{source}",
            "Change a source; same body as POST; a new file_pattern discards dug versions",
        ),
        (
            "DELETE",
            "/api/sources/{source}",
            "Delete a source, its versions, digs and fetched repository",
        ),
        (
            "GET",
            "/api/sources/{source}/versions",
            "Dug versions, oldest first: [{position, name, tag, commit, added, removed, modified}]",
        ),
        (
            "GET",
            "/api/sources/{source}/names?q=&limit=",
            "Symbol names starting with the query, shortest first",
        ),
        (
            "GET",
            "/api/sources/{source}/events?q=&kinds=&actions=&since=&until=&path=&after=&limit=",
            "Symbols added, removed or modified, newest version first; q is a regular expression over the whole name (a bare * means .*), case insensitive unless it has an uppercase letter; kinds is a comma list of function,define,variable; actions of added,removed,modified; since and until keep the versions after since and up to until (both version names); path keeps changes of definitions in files whose path starts with it (a file or a directory), or moved from there; after is the previous page's next cursor",
        ),
        (
            "GET",
            "/api/sources/{source}/symbols/{symbol}?path=",
            "Every event of one symbol, newest first; path as for events",
        ),
        (
            "GET",
            "/api/sources/{source}/symbols/{symbol}/code?version=&path=",
            "The symbol's definitions with their code in a version (default: the latest); a symbol defined in more than 25 files needs path, one of its files",
        ),
        (
            "GET",
            "/api/sources/{source}/symbols/{symbol}/revisions?path=",
            "Every version in which the code of the symbol's definitions changed, with that code; path as for code",
        ),
        (
            "GET",
            "/api/sources/{source}/digs",
            "The latest digs of a source, newest first, with their logs",
        ),
        (
            "POST",
            "/api/sources/{source}/digs?from=",
            "Queue a dig: fetch new tags and index the versions not dug yet; from=scratch first forgets every dug version",
        ),
        (
            "GET",
            "/api/digs/{dig}",
            "One dig with its status, progress and log",
        ),
        (
            "POST",
            "/api/preview",
            "List the versions a tag pattern selects in a repository without fetching it; body: {url, tag_pattern}",
        ),
    ];
    let endpoints: Vec<_> = ENDPOINTS
        .iter()
        .map(|(method, path, description)| json!({ "method": method, "path": path, "description": description }))
        .collect();
    Json(json!({
        "description": "SourceDigger shows when the functions, defines and variables of C projects were added, removed or changed across their versions. Errors are {\"error\": message} with a 4xx or 5xx status.",
        "endpoints": endpoints,
    }))
}

pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(error: E) -> Self {
        ApiError::Internal(error.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound(message) => (StatusCode::NOT_FOUND, message),
            ApiError::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            ApiError::Internal(error) => {
                tracing::error!("{error:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, format!("{error:#}"))
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

async fn with_database<T: Send + 'static>(
    state: &AppState,
    work: impl FnOnce(&Database) -> Result<T, ApiError> + Send + 'static,
) -> ApiResult<T> {
    let path = state.database_path.clone();
    let result = tokio::task::spawn_blocking(move || work(&Database::open(&path)?)).await?;
    result.map(Json)
}

fn find_source(database: &Database, name: &str) -> Result<Source, ApiError> {
    database
        .source(name)?
        .ok_or_else(|| ApiError::NotFound(format!("no source named {name}")))
}

fn bad_pattern(error: regex::Error) -> ApiError {
    ApiError::BadRequest(format!("invalid query: {error}"))
}

async fn list_sources(State(state): State<AppState>) -> ApiResult<Vec<SourceSummary>> {
    with_database(&state, |database| {
        let mut summaries = Vec::new();
        for source in database.sources()? {
            summaries.push(database.summarize(source)?);
        }
        Ok(summaries)
    })
    .await
}

async fn get_source(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<SourceSummary> {
    with_database(&state, move |database| {
        Ok(database.summarize(find_source(database, &name)?)?)
    })
    .await
}

fn check_settings(
    database: &Database,
    settings: &SourceSettings,
    source_id: Option<i64>,
) -> Result<(), ApiError> {
    settings
        .validate()
        .map_err(|error| ApiError::BadRequest(error.to_string()))?;
    match database.source(&settings.name)? {
        Some(other) if Some(other.id) != source_id => Err(ApiError::BadRequest(format!(
            "a source named {} already exists",
            settings.name
        ))),
        _ => Ok(()),
    }
}

async fn create_source(
    State(state): State<AppState>,
    Json(settings): Json<SourceSettings>,
) -> ApiResult<Source> {
    with_database(&state, move |database| {
        check_settings(database, &settings, None)?;
        Ok(database.create_source(settings)?)
    })
    .await
}

async fn update_source(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(settings): Json<SourceSettings>,
) -> ApiResult<Source> {
    with_database(&state, move |database| {
        let source = find_source(database, &name)?;
        check_settings(database, &settings, Some(source.id))?;
        Ok(database.update_source(source.id, settings)?)
    })
    .await
}

async fn delete_source(State(state): State<AppState>, Path(name): Path<String>) -> ApiResult<()> {
    let repositories = state.repositories.clone();
    with_database(&state, move |database| {
        let source = find_source(database, &name)?;
        database.delete_source(source.id)?;
        let mirror = Digger::mirror_path(&repositories, source.id);
        if mirror.exists() {
            std::fs::remove_dir_all(mirror)?;
        }
        Ok(())
    })
    .await
}

#[derive(Deserialize)]
struct NamesQuery {
    q: String,
    limit: Option<u32>,
}

async fn names(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<NamesQuery>,
) -> ApiResult<Vec<String>> {
    const DEFAULT_LIMIT: u32 = 20;
    with_database(&state, move |database| {
        let source = find_source(database, &name)?;
        let pattern = NamePattern::starting(&query.q).map_err(bad_pattern)?;
        Ok(database.names(source.id, &pattern, query.limit.unwrap_or(DEFAULT_LIMIT))?)
    })
    .await
}

#[derive(Deserialize)]
struct EventsQuery {
    q: String,
    kinds: Option<String>,
    actions: Option<String>,
    since: Option<String>,
    until: Option<String>,
    path: Option<String>,
    after: Option<String>,
    limit: Option<u32>,
}

fn split_list(list: Option<String>, all: &[&str]) -> Vec<String> {
    match list {
        Some(list) => list.split(',').map(str::to_string).collect(),
        None => all.iter().map(|item| item.to_string()).collect(),
    }
}

fn version_position(
    database: &Database,
    source: &Source,
    version: Option<&str>,
) -> Result<Option<i64>, ApiError> {
    let Some(version) = version else {
        return Ok(None);
    };
    match database.version_position(source.id, version)? {
        Some(position) => Ok(Some(position)),
        None => Err(ApiError::NotFound(format!(
            "no version {version} in {}",
            source.settings.name
        ))),
    }
}

async fn events(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<EventsQuery>,
) -> ApiResult<EventPage> {
    const DEFAULT_LIMIT: u32 = 200;
    with_database(&state, move |database| {
        let source = find_source(database, &name)?;
        let after = match query.after {
            Some(after) => Some(
                after
                    .parse()
                    .map_err(|error| ApiError::BadRequest(format!("{error:#}")))?,
            ),
            None => None,
        };
        let filter = EventFilter {
            names: NamePattern::exact(&query.q).map_err(bad_pattern)?,
            kinds: split_list(query.kinds, &Kind::ALL.map(Kind::as_str)),
            actions: split_list(query.actions, &Action::ALL.map(Action::as_str)),
            since: version_position(database, &source, query.since.as_deref())?,
            until: version_position(database, &source, query.until.as_deref())?,
            path: query.path,
            after,
            limit: query.limit.unwrap_or(DEFAULT_LIMIT),
        };
        Ok(database.events(&source, &filter)?)
    })
    .await
}

fn symbol_filter(symbol: &str) -> EventFilter {
    const MORE_EVENTS_THAN_ANY_NAME_HAS: u32 = 10_000_000;
    EventFilter {
        names: Some(NamePattern::literal(symbol)),
        kinds: split_list(None, &Kind::ALL.map(Kind::as_str)),
        actions: split_list(None, &Action::ALL.map(Action::as_str)),
        since: None,
        until: None,
        path: None,
        after: None,
        limit: MORE_EVENTS_THAN_ANY_NAME_HAS,
    }
}

#[derive(Deserialize)]
struct SymbolQuery {
    path: Option<String>,
}

async fn symbol(
    State(state): State<AppState>,
    Path((name, symbol)): Path<(String, String)>,
    Query(query): Query<SymbolQuery>,
) -> ApiResult<Vec<EventRecord>> {
    with_database(&state, move |database| {
        let source = find_source(database, &name)?;
        let filter = EventFilter {
            path: query.path,
            ..symbol_filter(&symbol)
        };
        Ok(database.events(&source, &filter)?.events)
    })
    .await
}

async fn list_versions(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Vec<VersionRecord>> {
    with_database(&state, move |database| {
        let source = find_source(database, &name)?;
        Ok(database.versions(source.id)?)
    })
    .await
}

struct SymbolContext {
    source: Source,
    versions: Vec<VersionRecord>,
    events: Vec<EventRecord>,
    repository: Repository,
}

fn symbol_context(
    database: &Database,
    repositories: &std::path::Path,
    name: &str,
    symbol: &str,
) -> Result<SymbolContext, ApiError> {
    let source = find_source(database, name)?;
    let versions = database.versions(source.id)?;
    let events = database.events(&source, &symbol_filter(symbol))?.events;
    let repository = Repository::open_bare(Digger::mirror_path(repositories, source.id))?;
    Ok(SymbolContext {
        source,
        versions,
        events,
        repository,
    })
}

#[derive(Deserialize)]
struct CodeQuery {
    version: Option<String>,
    path: Option<String>,
}

#[derive(Deserialize)]
struct RevisionsQuery {
    path: Option<String>,
}

fn checked_path(
    events: &[EventRecord],
    symbol: &str,
    path: Option<String>,
) -> Result<Option<String>, ApiError> {
    const MAXIMUM_FILES: usize = 25;
    if path.is_some() {
        return Ok(path);
    }
    let mut paths = BTreeSet::new();
    for event in events {
        paths.insert(&event.definition.path);
        if let Some(previous) = &event.previous {
            paths.insert(&previous.path);
        }
    }
    if paths.len() > MAXIMUM_FILES {
        return Err(ApiError::BadRequest(format!(
            "{symbol} is defined in {} files, more than {MAXIMUM_FILES}; choose one with path=",
            paths.len()
        )));
    }
    Ok(None)
}

async fn code(
    State(state): State<AppState>,
    Path((name, symbol)): Path<(String, String)>,
    Query(query): Query<CodeQuery>,
) -> ApiResult<Revision> {
    let repositories = state.repositories.clone();
    with_database(&state, move |database| {
        let context = symbol_context(database, &repositories, &name, &symbol)?;
        let version = match &query.version {
            Some(wanted) => context
                .versions
                .iter()
                .find(|version| &version.name == wanted),
            None => context.versions.last(),
        }
        .ok_or_else(|| ApiError::NotFound(format!("no version {:?} in {name}", query.version)))?;
        let path = checked_path(&context.events, &symbol, query.path)?;
        let history = History::new(
            &context.repository,
            &context.source,
            &symbol,
            &context.events,
            path.as_deref(),
        );
        Ok(history.at(version)?)
    })
    .await
}

async fn revisions(
    State(state): State<AppState>,
    Path((name, symbol)): Path<(String, String)>,
    Query(query): Query<RevisionsQuery>,
) -> ApiResult<Vec<Revision>> {
    let repositories = state.repositories.clone();
    with_database(&state, move |database| {
        let context = symbol_context(database, &repositories, &name, &symbol)?;
        let path = checked_path(&context.events, &symbol, query.path)?;
        let history = History::new(
            &context.repository,
            &context.source,
            &symbol,
            &context.events,
            path.as_deref(),
        );
        Ok(history.revisions(&context.versions)?)
    })
    .await
}

async fn list_digs(State(state): State<AppState>, Path(name): Path<String>) -> ApiResult<Vec<Dig>> {
    const LIMIT: u32 = 50;
    with_database(&state, move |database| {
        let source = find_source(database, &name)?;
        Ok(database.digs(source.id, LIMIT)?)
    })
    .await
}

#[derive(Deserialize)]
struct DigQuery {
    from: Option<DigScope>,
}

async fn start_dig(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<DigQuery>,
) -> ApiResult<Dig> {
    let wake = state.wake.clone();
    with_database(&state, move |database| {
        let source = find_source(database, &name)?;
        let from = query.from.unwrap_or(DigScope::New);
        let id = database.queue_dig(source.id, from)?;
        wake.send(())?;
        Ok(database.dig(id)?.expect("the dig was just queued"))
    })
    .await
}

async fn get_dig(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Dig> {
    with_database(&state, move |database| {
        database
            .dig(id)?
            .ok_or_else(|| ApiError::NotFound(format!("no dig {id}")))
    })
    .await
}

#[derive(Deserialize)]
struct PreviewRequest {
    url: String,
    tag_pattern: String,
}

#[derive(Serialize)]
struct PreviewVersion {
    name: String,
    tag: String,
}

async fn preview(Json(request): Json<PreviewRequest>) -> ApiResult<Vec<PreviewVersion>> {
    let tag_pattern = Regex::new(&request.tag_pattern)
        .map_err(|error| ApiError::BadRequest(format!("invalid tag pattern: {error}")))?;
    let versions = tokio::task::spawn_blocking(move || remote_versions(&request.url, &tag_pattern))
        .await?
        .map_err(|error| ApiError::BadRequest(format!("{error:#}")))?;
    Ok(Json(
        versions
            .into_iter()
            .map(|version| PreviewVersion {
                name: version.name,
                tag: version.tag,
            })
            .collect(),
    ))
}
