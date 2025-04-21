use color_eyre::eyre::Result as EyreResult;
use ssh_agent_mux::MuxAgent;
use tokio::select;
use tokio::signal::{self, unix::SignalKind};

mod cli;
mod logging;
mod service;

#[cfg(debug_assertions)]
fn install_eyre_hook() -> EyreResult<()> {
    color_eyre::config::HookBuilder::default()
        .display_env_section(true)
        .install()
}

#[cfg(not(debug_assertions))]
fn install_eyre_hook() -> EyreResult<()> {
    color_eyre::config::HookBuilder::default()
        .display_env_section(false)
        .install()
}

// Use current_thread to keep our resource utilization down; this program will generally be
// accessed by only one user, at the start of each SSH session, so it doesn't need tokio's powerful
// async multithreading
#[tokio::main(flavor = "current_thread")]
async fn main() -> EyreResult<()> {
    install_eyre_hook()?;

    let config = cli::Config::parse()?;

    // stdout logging doesn't strictly require holding the LoggerHandle, but better to not
    // ignore and drop it in case anyone adds file logging in the future
    let _logger = logging::setup_logger(config.log_level.into())?;

    if config.service.install_service || config.service.restart_service || config.service.uninstall_service {
        return service::handle_service_command(&config);
    }

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
