use std::{env, fs::File, io::Read, path::PathBuf};

use clap_serde_derive::{
    clap::{self, Parser, ValueEnum},
    serde::{self, Deserialize, Serialize},
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

#[derive(ClapSerde, Clone, Serialize)]
pub struct Config {
    /// Listen path
    #[default(PathBuf::from(concat!("~/.ssh/", env!("CARGO_PKG_NAME"), ".sock")))]
    #[arg(short, long = "listen")]
    pub listen_path: PathBuf,

    /// Log level for agent
    #[default(LogLevel::Warn)]
    #[arg(long, value_enum)]
    pub log_level: LogLevel,

    /// Optional log file for agent (logs to standard output, otherwise)
    #[arg(long, num_args = 1)]
    pub log_file: Option<PathBuf>,

    /// Agent sockets to multiplex
    #[arg()]
    pub agent_sock_paths: Vec<PathBuf>,

    // Following are part of command line args, but
    // not in configuration file
    /// Config file path (not an arg; copied from struct Args)
    #[arg(skip)]
    #[serde(skip_deserializing, skip_serializing)]
    pub config_path: PathBuf,

    #[serde(skip_deserializing, skip_serializing)]
    #[command(flatten)]
    pub service: service::ServiceArgs,
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

        config.config_path = args.config_path;
        config.listen_path = config.listen_path.expand_tilde_owned()?;
        config.log_file = config
            .log_file
            .map(|p| p.expand_tilde_owned())
            .transpose()?;
        config.agent_sock_paths = config
            .agent_sock_paths
            .into_iter()
            .map(|p| p.expand_tilde_owned())
            .collect::<Result<_, _>>()?;

        Ok(config)
    }
}

#[derive(ValueEnum, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    #[value(hide = true)]
    Trace = 5,
}

impl From<LogLevel> for LevelFilter {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Error => LevelFilter::Error,
            LogLevel::Warn => LevelFilter::Warn,
            LogLevel::Info => LevelFilter::Info,
            LogLevel::Debug => LevelFilter::Debug,
            LogLevel::Trace => LevelFilter::Trace,
        }
    }
}
