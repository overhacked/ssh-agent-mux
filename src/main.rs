use log::{LevelFilter, Log};
use ssh_agent_mux::MuxAgent;
use tokio::select;
use tokio::signal::{self, unix::SignalKind};

mod cli;

fn setup_logger(level: LevelFilter) -> Result<(), fern::InitError> {
    let env_log: Box<dyn Log> = Box::new(
        env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("off")
        ).build()
    );
    let cli_log = fern::Dispatch::new()
        .level(LevelFilter::Off)
        .level_for(env!("CARGO_CRATE_NAME"), level)
        .chain(std::io::stdout());

    fern::Dispatch::new()
        .chain(env_log)
        .chain(cli_log)
        .apply()?;

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = cli::Config::parse()?;

    setup_logger(config.log_level.into())?;

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
