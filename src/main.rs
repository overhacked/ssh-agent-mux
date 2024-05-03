#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sock_paths = std::env::args_os().skip(1);
    let identities = ssh_agent_mux::combine_identities(sock_paths).await?;
    dbg!(&identities);
    Ok(())
}
