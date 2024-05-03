use std::io::Error;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sock_path = std::env::args_os().nth(1).ok_or_else(|| Error::other("Socket path not specified"))?;
    ssh_agent_mux::list_identities(&sock_path).await
}
