use std::path::Path;
use std::env;

use flexi_logger::{
    filter::{LogLineFilter, LogLineWriter}, FileSpec, FlexiLoggerError, LogSpecification, Logger, LoggerHandle
};
use log::LevelFilter;

/// Suppress upstream extension failures by default, because we probe agents for the
/// session-bind@openssh.com extension and ignore failure. We'd like to keep the ability log
/// library errors but not cause lots of log noise on extension probing
struct SuppressExtensionFailure;
impl SuppressExtensionFailure {
    fn log_matches(message: &str) -> bool {
        !message.contains("Extension failure handling message")
    }
}

impl LogLineFilter for SuppressExtensionFailure {
    fn write(
        &self,
        now: &mut flexi_logger::DeferredNow,
        record: &log::Record,
        log_line_writer: &dyn LogLineWriter,
    ) -> std::io::Result<()> {
        let args = record.args();
        // Optimize for zero allocation iff the log message can be
        // accessed as a static string
        let should_log = if let Some(s) = args.as_str() {
            Self::log_matches(s)
        } else {
            Self::log_matches(&args.to_string())
        };
        if should_log {
            log_line_writer.write(now, record)
        } else {
            Ok(())
        }
    }
}

pub fn setup_logger(level: LevelFilter, log_file: Option<&Path>) -> Result<LoggerHandle, FlexiLoggerError> {
    // If RUST_LOG is in the environment, follow its directives;
    // otherwise, use the configuration file, command line args, or defaults.
    let logger = if env::var_os("RUST_LOG").is_some() {
        Logger::try_with_env()?
    } else {
        let logspec = LogSpecification::builder()
            .default(LevelFilter::Error)
            .module(env!("CARGO_CRATE_NAME"), level)
            .build();
        Logger::with(logspec).filter(Box::new(SuppressExtensionFailure))
    };

    if let Some(f) = log_file {
        let file_spec = FileSpec::try_from(f)?;
        logger.log_to_file(file_spec).start()
    } else {
        logger.log_to_stdout().start()
    }
}
