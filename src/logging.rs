use std::env;

use flexi_logger::{
    filter::{LogLineFilter, LogLineWriter},
    FlexiLoggerError, LogSpecification, Logger, LoggerHandle,
};
use log::LevelFilter;

/// Suppress upstream extension failures by default, because we probe agents for the
/// session-bind@openssh.com extension and ignore failure. We'd like to keep the ability log
/// library errors but not cause lots of log noise on extension probing
struct SuppressExtensionFailure;
impl LogLineFilter for SuppressExtensionFailure {
    fn write(
        &self,
        now: &mut flexi_logger::DeferredNow,
        record: &log::Record,
        log_line_writer: &dyn LogLineWriter,
    ) -> std::io::Result<()> {
        if !record
            .args()
            .to_string()
            .contains("Extension failure handling message")
        {
            log_line_writer.write(now, record)?;
        }
        Ok(())
    }
}

pub fn setup_logger(level: LevelFilter) -> Result<LoggerHandle, FlexiLoggerError> {
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

    logger.log_to_stdout().start()
}
