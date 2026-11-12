mod api;
mod db;
mod digger;
mod digs;
mod history;
mod query;
mod sources;
mod symbols;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use axum::Router;
use axum::http::HeaderValue;
use axum::http::header::CACHE_CONTROL;
use clap::Parser;
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tracing_subscriber::EnvFilter;

use crate::api::AppState;
use crate::db::Database;
use crate::digger::Digger;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(about = "Browse when the symbols of C projects were added, removed or changed")]
struct Arguments {
    #[arg(long, env = "SOURCEDIGGER_DATA_DIRECTORY", default_value_os_t = default_data_directory())]
    data_directory: PathBuf,
    #[arg(long, env = "SOURCEDIGGER_LISTEN", default_value = "127.0.0.1:2468")]
    listen: SocketAddr,
    #[arg(
        long,
        env = "SOURCEDIGGER_WEB_DIRECTORY",
        help = "Serve the built web interface from this directory"
    )]
    web_directory: Option<PathBuf>,
}

fn default_data_directory() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sourcedigger")
}

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
    let arguments = Arguments::parse();

    let repositories = arguments.data_directory.join("repositories");
    std::fs::create_dir_all(&repositories)
        .with_context(|| format!("creating {}", repositories.display()))?;
    let database_path = arguments.data_directory.join("sourcedigger.sqlite");
    Database::create(&database_path)?;

    let (wake, woken) = std::sync::mpsc::channel();
    let digger = Digger {
        database_path: database_path.clone(),
        repositories: repositories.clone(),
    };
    let worker = tokio::task::spawn_blocking(move || digger.run(woken));

    let mut router = api::router(AppState {
        database_path,
        repositories,
        wake,
    });
    if let Some(web_directory) = arguments.web_directory {
        router = router.merge(web_router(&web_directory));
    }
    router = router.layer(CompressionLayer::new());

    let listener = tokio::net::TcpListener::bind(arguments.listen).await?;
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
