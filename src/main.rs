use ssh_agent_mux::MuxAgent;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut socket_paths = std::env::args_os().skip(1);
    let listen_path = socket_paths.next().expect("Specify listen path");

    MuxAgent::run(&listen_path, socket_paths).await?;

    Ok(())
}
