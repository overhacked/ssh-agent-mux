use ssh_agent_mux::MuxAgent;

mod cli;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = cli::Config::parse()?;

    MuxAgent::run(&config.listen_path, &config.agent_sock_paths).await?;

    Ok(())
}
