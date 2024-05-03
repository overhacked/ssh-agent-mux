use std::{os::unix::net::UnixStream, path::Path};

use ssh_agent_lib::{client::connect, proto::Identity};

pub async fn list_identities(sock_path: impl AsRef<Path>) -> Result<Vec<Identity>, Box<dyn std::error::Error>> {
    let stream = UnixStream::connect(sock_path)?;
    let mut client = connect(stream.into()).await?;

    let identities = client.request_identities().await?;

    Ok(identities)
}

pub async fn combine_identities<I, P>(sock_paths: I) -> Result<Vec<Identity>, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut identities = vec![];
    for sock_path in sock_paths {
        identities.extend(list_identities(sock_path).await?);
    }

    Ok(identities)
}
