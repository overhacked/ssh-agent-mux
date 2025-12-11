use std::{
    env,
    ffi::OsString,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use clap_serde_derive::{
    clap::{self, Parser, ValueEnum},
    serde::{self, Deserialize, Serialize},
    ClapSerde,
};
use color_eyre::eyre::{bail, Result as EyreResult};
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

fn expand_path_env_tilde(mut path: PathBuf) -> EyreResult<PathBuf> {
    if path.as_os_str().as_encoded_bytes().contains(&b'$') {
        path = expand_path_env(&path)?;
    };

    if path.as_os_str().as_encoded_bytes().starts_with(b"~") {
        path = path.expand_tilde_owned()?;
    };

    Ok(path)
}

fn expand_path_env(path: impl AsRef<Path>) -> EyreResult<PathBuf> {
    let path = path.as_ref();
    let mut path_bytes_iter = path.as_os_str().as_encoded_bytes().iter().enumerate();
    let mut env_expanded_path = vec![];
    while let Some((i, b)) = path_bytes_iter.next() {
        if *b == b'$' {
            // Found '$', but previous character was backslash
            if let Some(b'\\') = env_expanded_path.last() {
                env_expanded_path.push(*b);
                continue;
            }
            let mut env_key = vec![];
            loop {
                match path_bytes_iter.next() {
                    // Duplicated dollar sign
                    Some((_, b'$')) => bail!("double dollar sign in variable: {}", path.display()),
                    // Opening '{', matches only immediately after '$'
                    Some((j, b'{')) if j == i + 1 => {}
                    // Opening '{' anywhere except following '$'
                    Some((_, b'{')) => {
                        bail!("double opening brace in variable: {}", path.display())
                    }
                    // Closing '}', error if it occurs before the opening '{' or if there are no
                    // non-brace characters after the opening '{'
                    Some((j, b'}')) if j <= i + 2 => {
                        bail!("premature closing brace in variable: {}", path.display())
                    }
                    // Closing '}'
                    Some((_, b'}')) => break,
                    // Any non-brace character before the opening '{'
                    Some((j, _)) if j == i + 1 => bail!(
                        "syntax error, no opening brace after dollar sign: {}",
                        path.display()
                    ),
                    // Any non-brace character after the opening '{'
                    Some((_, b)) => env_key.push(*b),
                    // End of string before finding the closing '}'
                    None => bail!(
                        "syntax error, end of string while parsing variable: {}",
                        path.display()
                    ),
                }
            }
            // SAFETY: all bytes in env_key come from path.as_os_str().as_encoded_bytes()
            let env_key = unsafe { OsString::from_encoded_bytes_unchecked(env_key) };
            let Some(env_value) = env::var_os(&env_key) else {
                bail!("{env_key:?} not set in the environment")
            };
            env_expanded_path.extend(env_value.as_encoded_bytes());
        } else {
            env_expanded_path.push(*b);
        }
    }
    // SAFETY: all bytes in env_expanded_path come from either
    // path.as_os_str().as_encoded_bytes() or env::var_os().as_encoded_bytes()
    let env_expanded_path = unsafe { OsString::from_encoded_bytes_unchecked(env_expanded_path) };

    Ok(PathBuf::from(env_expanded_path))
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
    #[serde(skip)]
    pub config_path: PathBuf,

    #[serde(skip)]
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
            .map(expand_path_env_tilde)
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

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_ENV_VAR: &str = "CARGO_TEST_EXAMPLE";
    const FAKE_SOCK_PATH: &str = "/path/to/nonexistent/socket.sock";

    #[test]
    fn expand_env_variable() {
        env::set_var(FAKE_ENV_VAR, FAKE_SOCK_PATH);
        let to_be_expanded = PathBuf::from(format!("${{{FAKE_ENV_VAR}}}"));
        let expected = PathBuf::from(FAKE_SOCK_PATH);
        let actual = expand_path_env(to_be_expanded).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn expand_env_multiple() {
        env::set_var("CARGO_TEST_VAR_1", "one");
        env::set_var("CARGO_TEST_VAR_2", "two");
        let to_be_expanded = PathBuf::from("{${CARGO_TEST_VAR_1}.${CARGO_TEST_VAR_2}}");
        let expected = PathBuf::from("{one.two}");
        let actual = expand_path_env(to_be_expanded).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn expand_env_escaping() {
        env::set_var(FAKE_ENV_VAR, FAKE_SOCK_PATH);
        let to_be_expanded = PathBuf::from(format!("\\${{{FAKE_ENV_VAR}}}"));
        let expected = to_be_expanded.clone();
        let actual = expand_path_env(to_be_expanded).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn expand_env_catch_errors() {
        env::set_var(FAKE_ENV_VAR, FAKE_SOCK_PATH);
        // Use valid environment variable in bad expansions to catch unintentional success
        let bad_expansions = [
            // (to_be_expanded, expected_error,)
            (format!("$${{{FAKE_ENV_VAR}}}"), "double dollar sign"),
            (format!("${{{FAKE_ENV_VAR}$}}"), "double dollar sign"),
            (format!("${{{FAKE_ENV_VAR}}}$"), "end of string"),
            (format!("${{{{{FAKE_ENV_VAR}}}"), "double opening brace"),
            (format!("${{{FAKE_ENV_VAR}{{}}"), "double opening brace"),
            (format!("${{}}{FAKE_ENV_VAR}}}"), "premature closing brace"),
            ("${}".to_owned(), "premature closing brace"),
            (format!("${FAKE_ENV_VAR}"), "no opening brace"),
            (format!("${{{FAKE_ENV_VAR}"), "end of string"),
            ("${ZZXZ_NOT_IN_ENVIRONMENT_QZZ}".to_owned(), "not set"),
        ];
        for (to_be_expanded, expected_error) in bad_expansions {
            let to_be_expanded = PathBuf::from(to_be_expanded);
            let actual_result = expand_path_env(&to_be_expanded);
            assert!(
                actual_result.is_err(),
                "expected error, but expansion succeeded: {to_be_expanded:?}"
            );

            let error_msg = actual_result.unwrap_err().to_string();
            assert!(
                error_msg.contains(expected_error),
                "error message for expansion ({to_be_expanded:?}) did not contain {expected_error:?}: {error_msg}"
            );
        }
    }
}
