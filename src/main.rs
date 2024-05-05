use ssh_agent_mux::MuxAgent;
use tokio::select;
use tokio::signal::{self, unix::SignalKind};

mod cli;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let config = cli::Config::parse()?;

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
