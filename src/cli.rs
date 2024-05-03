use std::{env, fs::File, io::Read, path::PathBuf};

use clap_serde_derive::{
    clap::{self, Parser},
    ClapSerde,
};

fn default_config_path() -> PathBuf {
    let config_dir = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|v| PathBuf::from(v).join(".config")))
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
    #[arg(short, long = "listen")]
    pub listen_path: PathBuf,

    /// Agent sockets to multiplex
    #[arg()]
    pub agent_sock_paths: Vec<PathBuf>,
}

impl Config {
    pub fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        let mut args = Args::parse();

        let config = if let Ok(mut f) = File::open(&args.config_path) {
            let mut config_text = String::new();
            f.read_to_string(&mut config_text)?;
            match toml::from_str::<<Config as ClapSerde>::Opt>(&config_text) {
                Ok(config) => Config::from(config).merge(&mut args.config),
                Err(_) => todo!("Error for config"),
            }
        } else {
            Config::from(&mut args.config)
        };

        Ok(config)
    }
}
