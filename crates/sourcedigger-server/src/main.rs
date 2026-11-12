mod api;
mod db;
mod digger;
mod digs;
mod history;
mod query;
mod settings;
mod sources;
mod symbols;

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use axum::Router;
use axum::http::HeaderValue;
use axum::http::header::CACHE_CONTROL;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tracing_subscriber::EnvFilter;

use crate::api::AppState;
use crate::db::Database;
use crate::digger::Digger;
use crate::settings::Settings;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn web_router(directory: &Path) -> Router {
    const ASSETS: &str = "assets";
    const IMMUTABLE: HeaderValue = HeaderValue::from_static("public, max-age=31536000, immutable");
    let assets = Router::new()
        .nest_service(&format!("/{ASSETS}"), ServeDir::new(directory.join(ASSETS)))
        .layer(SetResponseHeaderLayer::overriding(CACHE_CONTROL, IMMUTABLE));
    let index = ServeFile::new(directory.join("index.html"));
    assets.fallback_service(ServeDir::new(directory).fallback(index))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("sourcedigger_server=info".parse()?),
        )
        .with_writer(std::io::stderr)
        .init();
    let settings = Settings::load()?;
    rayon::ThreadPoolBuilder::new()
        .num_threads(settings.threads)
        .build_global()?;

    std::fs::create_dir_all(&settings.repositories)
        .with_context(|| format!("creating {}", settings.repositories.display()))?;
    if let Some(directory) = settings.database.parent() {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("creating {}", directory.display()))?;
    }
    Database::create(&settings.database)?;

    let (wake, woken) = std::sync::mpsc::channel();
    let digger = Digger {
        settings: settings.clone(),
    };
    let worker = tokio::task::spawn_blocking(move || digger.run(woken));

    let mut router = api::router(AppState {
        settings: Arc::new(settings.clone()),
        wake,
    });
    if let Some(web_directory) = &settings.web_directory {
        router = router.merge(web_router(web_directory));
    }
    router = router.layer(CompressionLayer::new());

    let listener = tokio::net::TcpListener::bind(settings.listen).await?;
    println!("Listening on http://{}", listener.local_addr()?);
    tokio::select! {
        served = axum::serve(listener, router) => served?,
        worked = worker => {
            worked??;
            bail!("the digger stopped");
        }
    }
    Ok(())
}
