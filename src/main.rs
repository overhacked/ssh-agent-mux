use std::env;

use flexi_logger::{filter::{LogLineFilter, LogLineWriter}, FlexiLoggerError, LogSpecification, Logger, LoggerHandle};
use log::LevelFilter;
use ssh_agent_mux::MuxAgent;
use tokio::select;
use tokio::signal::{self, unix::SignalKind};

mod cli;

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
        if !record.args().to_string().contains("Extension failure handling message") {
            log_line_writer.write(now, record)?;
        }
        Ok(())
    }
}

fn setup_logger(level: LevelFilter) -> Result<LoggerHandle, FlexiLoggerError> {
    // If RUST_LOG is in the environment, follow its directives;
    // otherwise, use the configuration file, command line args, or defaults.
    let logger = if env::var_os("RUST_LOG").is_some() {
        Logger::try_with_env()?
    } else {
        let logspec = LogSpecification::builder()
            .default(LevelFilter::Error)
            .module(env!("CARGO_CRATE_NAME"), level)
            .build();
        Logger::with(logspec)
            .filter(Box::new(SuppressExtensionFailure))
    };

    logger.log_to_stdout().start()
}

// Use current_thread to keep our resource utilization down; this program will generally be
// accessed by only one user, at the start of each SSH session, so it doesn't need tokio's powerful
// async multithreading
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = cli::Config::parse()?;

    // stdout logging doesn't strictly require holding the LoggerHandle, but better to not
    // ignore and drop it in case anyone adds file logging in the future
    let _logger = setup_logger(config.log_level.into())?;

    let mut sigterm = signal::unix::signal(SignalKind::terminate())?;

    select! {
        res = MuxAgent::run(&config.listen_path, &config.agent_sock_paths) => res?,
        // Cleanly exit on interrupt and SIGTERM, allowing
        // MuxAgent to clean up
        _ = signal::ctrl_c() => (),
        Some(_) = sigterm.recv() => (),
    }

    Ok(())
}
