use std::{env, fs::File, io::Read, path::PathBuf};

use clap_serde_derive::{
    clap::{self, Parser, ValueEnum},
    serde::{self, Deserialize},
    ClapSerde,
};
use color_eyre::eyre::Result as EyreResult;
use expand_tilde::ExpandTilde;
use log::LevelFilter;

use crate::service;

fn default_config_path() -> PathBuf {
    let config_dir = env::var_os("XDG_CONFIG_HOME")
        .or_else(|| Some("~/.config".into()))
        .map(PathBuf::from)
        .and_then(|p| p.expand_tilde_owned().ok())
        .expect("HOME not defined in environment");

    config_dir
        .join(env!("CARGO_PKG_NAME"))
        .join(concat!(env!("CARGO_PKG_NAME"), ".toml"))
}

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    /// Config file
    #[arg(short, long = "config", default_value_os_t = default_config_path())]
    config_path: PathBuf,

    /// Config from file or args
    #[command(flatten)]
    config: <Config as ClapSerde>::Opt,
}

#[derive(ClapSerde)]
pub struct Config {
    /// Listen path
    #[default(PathBuf::from(concat!("~/.ssh/", env!("CARGO_PKG_NAME"), ".sock")))]
    #[arg(short, long = "listen")]
    pub listen_path: PathBuf,

    /// Log level for agent
    #[default(LogLevel::Warn)]
    #[arg(long, value_enum)]
    pub log_level: LogLevel,

    #[serde(skip_deserializing)]
    #[command(flatten)]
    pub service: service::ServiceArgs,

    /// Agent sockets to multiplex
    #[arg()]
    pub agent_sock_paths: Vec<PathBuf>,
}

impl Config {
    pub fn parse() -> EyreResult<Self> {
        let mut args = Args::parse();

        let mut config = if let Ok(mut f) = File::open(&args.config_path) {
            log::info!("Read configuration from {}", args.config_path.display());
            let mut config_text = String::new();
            f.read_to_string(&mut config_text)?;
            let file_config = toml::from_str::<<Config as ClapSerde>::Opt>(&config_text)?;
            Config::from(file_config).merge(&mut args.config)
        } else {
            Config::from(&mut args.config)
        };

        config.listen_path = config.listen_path.expand_tilde_owned()?;
        config.agent_sock_paths = config
            .agent_sock_paths
            .into_iter()
            .map(|p| p.expand_tilde_owned())
            .collect::<Result<_, _>>()?;

        Ok(config)
    }
}

#[derive(ValueEnum, Clone, Deserialize)]
pub enum LogLevel {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
}

impl From<LogLevel> for LevelFilter {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Error => LevelFilter::Error,
            LogLevel::Warn => LevelFilter::Warn,
            LogLevel::Info => LevelFilter::Info,
            LogLevel::Debug => LevelFilter::Debug,
        }
    }
}
