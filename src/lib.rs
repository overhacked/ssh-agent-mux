use std::{os::unix::net::UnixStream, path::Path};

use ssh_agent_lib::client::connect;

pub async fn list_identities(sock_path: impl AsRef<Path>) -> Result<(), Box<dyn std::error::Error>> {
    let stream = UnixStream::connect(sock_path)?;
    let mut client = connect(stream.into()).await?;

    eprintln!(
        "Identities that this agent knows of: {:#?}",
        client.request_identities().await?
    );

    Ok(())
}
