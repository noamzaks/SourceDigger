use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Args, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    #[arg(long, env = "SOURCEDIGGER_DATABASE")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<PathBuf>,
    #[arg(long, env = "SOURCEDIGGER_REPOSITORIES")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repositories: Option<PathBuf>,
    #[arg(long, env = "SOURCEDIGGER_THREADS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<usize>,
    #[arg(long, env = "SOURCEDIGGER_LISTEN")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listen: Option<SocketAddr>,
    #[arg(long, env = "SOURCEDIGGER_WEB_DIRECTORY")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_directory: Option<PathBuf>,
}

impl Options {
    pub fn read(file: &Path) -> Result<Self> {
        if !file.exists() {
            return Ok(Self::default());
        }
        let text =
            std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing {}", file.display()))
    }

    pub fn write(&self, file: &Path) -> Result<()> {
        if self.threads == Some(0) {
            bail!("threads must be at least 1");
        }
        if let Some(directory) = file.parent() {
            std::fs::create_dir_all(directory)
                .with_context(|| format!("creating {}", directory.display()))?;
        }
        std::fs::write(file, toml::to_string(self)?)
            .with_context(|| format!("writing {}", file.display()))
    }
}

#[derive(Parser)]
#[command(about = "Browse when the symbols of C projects were added, removed or changed")]
struct Arguments {
    #[arg(
        long,
        env = "SOURCEDIGGER_SETTINGS",
        default_value_os_t = home(dirs::config_dir()).join("settings.toml"),
        help = "Site settings, edited on the site settings page; options override them"
    )]
    settings: PathBuf,
    #[command(flatten)]
    options: Options,
}

#[derive(Clone, Debug, Serialize)]
pub struct Settings {
    pub file: PathBuf,
    pub overridden: Options,
    pub database: PathBuf,
    pub repositories: PathBuf,
    pub threads: usize,
    pub listen: SocketAddr,
    pub web_directory: Option<PathBuf>,
}

impl Settings {
    const DEFAULT_LISTEN: &str = "127.0.0.1:2468";

    pub fn load() -> Result<Self> {
        let arguments = Arguments::parse();
        let saved = Options::read(&arguments.settings)?;
        let overridden = arguments.options;
        let data = home(dirs::data_dir());
        Ok(Self {
            database: overridden
                .database
                .clone()
                .or(saved.database)
                .unwrap_or_else(|| data.join("sourcedigger.sqlite")),
            repositories: overridden
                .repositories
                .clone()
                .or(saved.repositories)
                .unwrap_or_else(|| data.join("repositories")),
            threads: overridden
                .threads
                .or(saved.threads)
                .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from)),
            listen: match overridden.listen.or(saved.listen) {
                Some(listen) => listen,
                None => Self::DEFAULT_LISTEN.parse()?,
            },
            web_directory: overridden.web_directory.clone().or(saved.web_directory),
            file: arguments.settings,
            overridden,
        })
    }
}

fn home(base: Option<PathBuf>) -> PathBuf {
    base.unwrap_or_else(|| PathBuf::from("."))
        .join("sourcedigger")
}
