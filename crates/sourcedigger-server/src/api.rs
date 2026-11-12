use std::collections::BTreeSet;
use std::sync::Arc;
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
use crate::history::{Change, History, Revision, changes as file_changes};
use crate::query::{NamePattern, Search, Term};
use crate::settings::{Options, Settings};
use crate::sources::{Source, SourceSettings, SourceSummary};
use crate::symbols::{EventFilter, EventPage, EventRecord, VersionRecord};

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub wake: Sender<()>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api", get(index))
        .route("/llms.txt", get(llms_txt))
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
        .route(
            "/api/sources/{source}/symbols/{symbol}/changes",
            get(changes),
        )
        .route("/api/sources/{source}/digs", get(list_digs).post(start_dig))
        .route("/api/digs/{dig}", get(get_dig))
        .route("/api/preview", post(preview))
        .route("/api/site", get(site).put(save_site))
        .with_state(state)
}

#[derive(Serialize)]
struct Site {
    running: Settings,
    saved: Options,
}

async fn site(State(state): State<AppState>) -> ApiResult<Site> {
    Ok(Json(Site {
        running: state.settings.as_ref().clone(),
        saved: Options::read(&state.settings.file)?,
    }))
}

async fn save_site(State(state): State<AppState>, Json(saved): Json<Options>) -> ApiResult<Site> {
    saved
        .write(&state.settings.file)
        .map_err(|error| ApiError::BadRequest(format!("{error:#}")))?;
    Ok(Json(Site {
        running: state.settings.as_ref().clone(),
        saved,
    }))
}

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
        "Dug versions, oldest first: [{position, name, tag, commit, added, removed, modified, signature}], with the number of events of each action",
    ),
    (
        "GET",
        "/api/sources/{source}/names?q=&limit=",
        "Symbol names starting with the query, shortest first",
    ),
    (
        "GET",
        "/api/sources/{source}/events?q=&cursor=&limit=",
        "Changes of symbols, newest version first, as {query, events, next}; q is a search (see below), the same text as the q of a search page's URL, and query is how it was read; cursor is the previous page's next",
    ),
    (
        "GET",
        "/api/sources/{source}/symbols/{symbol}?path=",
        "Every event of one symbol, newest first; path keeps the definitions in files whose path starts with it",
    ),
    (
        "GET",
        "/api/sources/{source}/symbols/{symbol}/code?version=&path=",
        "The symbol's definitions with their code in a version (default: the latest), of every kind: a function next to the invocations naming it, such as EXPORT_SYMBOL_GPL(name); files maps each file to its code, without definitions repeated inside others (a struct inside a variable's declaration); a symbol defined in more than 25 files needs path, one of its files",
    ),
    (
        "GET",
        "/api/sources/{source}/symbols/{symbol}/revisions?path=",
        "Every version in which the code of the symbol's definitions changed, with that code; path as for code",
    ),
    (
        "GET",
        "/api/sources/{source}/symbols/{symbol}/changes?path=",
        "The same versions, newest first, as {version, previous_version, action, files: [{path, moved_from, before, after}]}: each file's code before and after, with moved_from when it moved; path as for code",
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
        "GET",
        "/api/site",
        "Site settings: running (in use, with the file and what options override) and saved (in the settings file)",
    ),
    (
        "PUT",
        "/api/site",
        "Save site settings to the settings file, applied on restart; body: {database, repositories, threads, listen, web_directory}, all optional",
    ),
    (
        "POST",
        "/api/preview",
        "List the versions a tag pattern selects in a repository without fetching it; body: {url, tag_pattern}",
    ),
];

const DESCRIPTION: &str = "SourceDigger shows when the symbols of C projects (functions, defines, variables, types and file-scope invocations) were added, removed or changed across their versions. Everything the web interface shows comes from this JSON API; errors are {\"error\": message} with a 4xx or 5xx status.";

const SEARCH: &str = "A search is terms, as typed in the web search box: kmalloc* -kmalloc_node* kind:function path:mm/ -path:mm/kasan/. A term is a field and a value (field:value), or a name. Names and paths are regular expressions (a bare * means .*, a bare + means .+): a name matches all of it, case insensitive unless it has an uppercase letter, and a path its start, so path:mm/ is a directory. Terms side by side must all match, except that terms of the same field mean any of them: path:mm/ path:kernel/. AND, OR, NOT (or -) and parentheses combine terms: (kind:function action:added) OR NOT path:mm/. NOT binds tightest, then terms side by side, then AND, then OR. after and before keep the versions strictly between them. Quote values with spaces, a name starting with (, or a name that is AND, OR or NOT: path:\"my dir/\". The query of a page is the search as read: a tree of {\"term\": {field, value}}, {\"not\": ...}, {\"and\": [...]} and {\"or\": [...]}; an empty search is {\"and\": []}.";

const LIMITATIONS: &[&str] = &[
    "Only C is read, with tree-sitter and without a preprocessor: every #if branch is indexed, macros are not expanded, and code tree-sitter cannot parse (such as an #if inside an expression) can hide the file-scope definitions after it that are indented.",
    "Function prototypes and extern declarations are not indexed, so where a function is declared (in a header) and how that declaration changed are not shown.",
    "Comments are not indexed: neither the documentation above a definition nor license headers such as SPDX-License-Identifier.",
    "References are not indexed: there is no list of the callers or users of a symbol.",
    "Definitions written through a macro, such as SYSCALL_DEFINE3(open, ...) { ... }, are not read as functions; they appear as an invocation named after the macro's first identifier argument (open).",
];

const PAGES: &[(&str, &str)] = &[
    ("/", "GET /api/sources"),
    (
        "/sources/{source}?q=",
        "GET /api/sources/{source}/events?q= with the same q, and /versions",
    ),
    (
        "/sources/{source}/symbols/{symbol}",
        "GET /api/sources/{source}/symbols/{symbol} and /revisions",
    ),
    (
        "/sources/{source}/symbols/{symbol}/code?version=",
        "GET /api/sources/{source}/symbols/{symbol}/code?version=",
    ),
];

fn kind_meaning(kind: Kind) -> &'static str {
    match kind {
        Kind::Function => "a function definition",
        Kind::Define => "a #define",
        Kind::Variable => "a variable defined at file scope",
        Kind::Struct => "a struct with its fields",
        Kind::Union => "a union with its fields",
        Kind::Enum => "an enum with its enumerators",
        Kind::Enumerator => "one constant of an enum",
        Kind::Typedef => "a typedef",
        Kind::Invocation => {
            "a macro or function called at file scope, named after its first identifier argument (EXPORT_SYMBOL_GPL(foo) is foo's) or else after itself (MODULE_LICENSE(\"GPL\"))"
        }
    }
}

fn action_meaning(action: Action) -> &'static str {
    match action {
        Action::Added => "the definition appeared",
        Action::Removed => "the definition disappeared",
        Action::Modified => {
            "the code changed but the signature did not (a body, or a struct's fields), or it moved to another file"
        }
        Action::Signature => "the signature changed",
    }
}

async fn index() -> Json<serde_json::Value> {
    let endpoints: Vec<_> = ENDPOINTS
        .iter()
        .map(|(method, path, description)| json!({ "method": method, "path": path, "description": description }))
        .collect();
    let kinds: serde_json::Map<_, _> = Kind::ALL
        .iter()
        .map(|kind| (kind.as_str().to_string(), json!(kind_meaning(*kind))))
        .collect();
    let actions: serde_json::Map<_, _> = Action::ALL
        .iter()
        .map(|action| (action.as_str().to_string(), json!(action_meaning(*action))))
        .collect();
    let fields: serde_json::Map<_, _> = Search::FIELDS
        .iter()
        .map(|(field, meaning)| (field.to_string(), json!(meaning)))
        .collect();
    Json(json!({
        "description": DESCRIPTION,
        "endpoints": endpoints,
        "search": SEARCH,
        "fields": fields,
        "kinds": kinds,
        "actions": actions,
        "limitations": LIMITATIONS,
    }))
}

async fn llms_txt() -> ([(axum::http::HeaderName, &'static str); 1], String) {
    let mut text = format!(
        "# SourceDigger\n\n> {DESCRIPTION}\n\n`GET /api` returns this as JSON. Paths are relative to this server.\n\n## Endpoints\n\n"
    );
    for (method, path, description) in ENDPOINTS {
        text.push_str(&format!("- `{method} {path}`: {description}\n"));
    }
    text.push_str(&format!("\n## Search\n\n{SEARCH}\n\n"));
    for (field, meaning) in Search::FIELDS {
        text.push_str(&format!("- `{field}:`: {meaning}\n"));
    }
    text.push_str("\n## Kinds\n\n");
    for kind in Kind::ALL {
        text.push_str(&format!("- `{}`: {}\n", kind.as_str(), kind_meaning(kind)));
    }
    text.push_str("\n## Actions\n\n");
    for action in Action::ALL {
        text.push_str(&format!(
            "- `{}`: {}\n",
            action.as_str(),
            action_meaning(action)
        ));
    }
    text.push_str("\n## Pages\n\nThe web pages need JavaScript; each one shows what these endpoints return:\n\n");
    for (page, endpoints) in PAGES {
        text.push_str(&format!("- `{page}`: {endpoints}\n"));
    }
    text.push_str("\n## Limitations\n\n");
    for limitation in LIMITATIONS {
        text.push_str(&format!("- {limitation}\n"));
    }
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/markdown; charset=utf-8",
        )],
        text,
    )
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
    let path = state.settings.database.clone();
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
    let repositories = state.settings.repositories.clone();
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
    #[serde(default)]
    q: String,
    cursor: Option<String>,
    limit: Option<u32>,
}

#[derive(Serialize)]
struct SearchPage {
    query: Search,
    #[serde(flatten)]
    page: EventPage,
}

fn check_versions(database: &Database, source: &Source, search: &Search) -> Result<(), ApiError> {
    const VERSION_FIELDS: [&str; 3] = ["version", "after", "before"];
    for term in search.terms() {
        if !VERSION_FIELDS.contains(&term.field.as_str()) {
            continue;
        }
        if database.version_position(source.id, &term.value)?.is_none() {
            return Err(ApiError::NotFound(format!(
                "no version {} in {}",
                term.value, source.settings.name
            )));
        }
    }
    Ok(())
}

async fn events(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<EventsQuery>,
) -> ApiResult<SearchPage> {
    const DEFAULT_LIMIT: u32 = 200;
    with_database(&state, move |database| {
        let source = find_source(database, &name)?;
        let cursor = match query.cursor {
            Some(cursor) => Some(
                cursor
                    .parse()
                    .map_err(|error| ApiError::BadRequest(format!("{error:#}")))?,
            ),
            None => None,
        };
        let search = Search::parse(&query.q).map_err(ApiError::BadRequest)?;
        check_versions(database, &source, &search)?;
        let filter = EventFilter {
            names: search.names(),
            search,
            cursor,
            limit: query.limit.unwrap_or(DEFAULT_LIMIT),
        };
        let page = database.events(&source, &filter)?;
        Ok(SearchPage {
            query: filter.search,
            page,
        })
    })
    .await
}

fn symbol_filter(symbol: &str) -> EventFilter {
    const MORE_EVENTS_THAN_ANY_NAME_HAS: u32 = 10_000_000;
    EventFilter {
        names: Some(NamePattern::literal(symbol)),
        search: Search::everything(),
        cursor: None,
        limit: MORE_EVENTS_THAN_ANY_NAME_HAS,
    }
}

#[derive(Deserialize)]
struct PathQuery {
    path: Option<String>,
}

async fn symbol(
    State(state): State<AppState>,
    Path((name, symbol)): Path<(String, String)>,
    Query(query): Query<PathQuery>,
) -> ApiResult<Vec<EventRecord>> {
    with_database(&state, move |database| {
        let source = find_source(database, &name)?;
        let filter = EventFilter {
            search: match query.path {
                Some(path) => Search::Term(Term::new("path", &regex::escape(&path))),
                None => Search::everything(),
            },
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

fn history(
    database: &Database,
    repositories: &std::path::Path,
    name: &str,
    symbol: &str,
    path: Option<String>,
) -> Result<History, ApiError> {
    let source = find_source(database, name)?;
    let events = database.events(&source, &symbol_filter(symbol))?.events;
    let path = checked_path(&events, symbol, path)?;
    let repository = Repository::open_bare(Digger::mirror_path(repositories, source.id))?;
    Ok(History::new(
        repository,
        source,
        symbol,
        &events,
        path.as_deref(),
    ))
}

#[derive(Deserialize)]
struct CodeQuery {
    version: Option<String>,
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
    let repositories = state.settings.repositories.clone();
    with_database(&state, move |database| {
        let history = history(database, &repositories, &name, &symbol, query.path)?;
        let versions = database.versions(history.source.id)?;
        let version = match &query.version {
            Some(wanted) => versions.iter().find(|version| &version.name == wanted),
            None => versions.last(),
        }
        .ok_or_else(|| ApiError::NotFound(format!("no version {:?} in {name}", query.version)))?;
        Ok(history.at(version)?)
    })
    .await
}

async fn revisions(
    State(state): State<AppState>,
    Path((name, symbol)): Path<(String, String)>,
    Query(query): Query<PathQuery>,
) -> ApiResult<Vec<Revision>> {
    let repositories = state.settings.repositories.clone();
    with_database(&state, move |database| {
        let history = history(database, &repositories, &name, &symbol, query.path)?;
        Ok(history.revisions(&database.versions(history.source.id)?)?)
    })
    .await
}

async fn changes(
    State(state): State<AppState>,
    Path((name, symbol)): Path<(String, String)>,
    Query(query): Query<PathQuery>,
) -> ApiResult<Vec<Change>> {
    let repositories = state.settings.repositories.clone();
    with_database(&state, move |database| {
        let history = history(database, &repositories, &name, &symbol, query.path)?;
        let revisions = history.revisions(&database.versions(history.source.id)?)?;
        Ok(file_changes(&revisions))
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
