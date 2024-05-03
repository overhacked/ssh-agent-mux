use std::{os::unix::net::UnixStream, path::{Path, PathBuf}};

use ssh_agent_lib::{agent::{ListeningSocket, Session}, client::connect, error::AgentError, proto::Identity, Agent};
use tokio::net::UnixListener;

pub async fn list_identities(sock_path: impl AsRef<Path>) -> Result<Vec<Identity>, AgentError> {
    let stream = UnixStream::connect(sock_path)?;
    let mut client = connect(stream.into()).await
        .map_err(|_| AgentError::Other(Box::<dyn std::error::Error + Send + Sync>::from("Failed to connect to agent")))?;

    let identities = client.request_identities().await?;

    Ok(identities)
}

pub async fn combine_identities<I, P>(sock_paths: I) -> Result<Vec<Identity>, AgentError>
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

struct MuxAgentSession {
    socket_paths: Vec<PathBuf>,
}

#[ssh_agent_lib::async_trait]
impl Session for MuxAgentSession {
    async fn request_identities(&mut self) -> Result<Vec<Identity>, AgentError> {
        combine_identities(&self.socket_paths).await
    }
}

pub struct MuxAgent {
    socket_paths: Vec<PathBuf>,
}

impl MuxAgent {
    pub async fn run<I, P>(listen_sock: impl AsRef<Path>, agent_socks: I) -> Result<(), AgentError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let socket_paths = agent_socks.into_iter()
            .map(|p| p.as_ref().to_path_buf())
            .collect();
        let this = Self {
            socket_paths,
        };
        this.listen(SelfDeletingUnixListener::bind(listen_sock)?).await?;

        Ok(())
    }
}

impl Agent for MuxAgent {
    #[doc = "Create new session object when a new socket is accepted."]
    fn new_session(&mut self) -> impl Session {
        MuxAgentSession {
            // TODO: should there be a connection pool?
            socket_paths: self.socket_paths.clone(),
        }
    }
}

#[derive(Debug)]
struct SelfDeletingUnixListener {
    path: PathBuf,
    listener: UnixListener,
}

impl SelfDeletingUnixListener {
    fn bind(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        UnixListener::bind(&path)
            .map(|listener| Self { path, listener, })
    }
}

impl Drop for SelfDeletingUnixListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[ssh_agent_lib::async_trait]
impl ListeningSocket for SelfDeletingUnixListener {
    type Stream = tokio::net::UnixStream;

    async fn accept(&mut self) -> std::io::Result<Self::Stream> {
        UnixListener::accept(&self.listener).await.map(|(s, _addr)| s)
    }
}
