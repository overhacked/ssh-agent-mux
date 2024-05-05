use std::{
    collections::HashMap,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
};

use ssh_agent_lib::{
    agent::{ListeningSocket, Session},
    client::connect,
    error::AgentError,
    proto::{Identity, SignRequest},
    Agent,
};
use ssh_key::{public::KeyData as PubKeyData, Signature};
use tokio::net::UnixListener;

type KnownPubKeys = Arc<Mutex<HashMap<PubKeyData, PathBuf>>>;

struct MuxAgentSession {
    socket_paths: Vec<PathBuf>,
    known_keys: KnownPubKeys,
}

impl MuxAgentSession {
    async fn connect_upstream_agent(
        &self,
        sock_path: impl AsRef<Path>,
    ) -> Result<Pin<Box<dyn Session>>, AgentError> {
        let stream = UnixStream::connect(sock_path)?;
        connect(stream.into())
            .await
            .map_err(|e| AgentError::Other(format!("Failed to connect to agent: {e}").into()))
    }
}

#[ssh_agent_lib::async_trait]
impl Session for MuxAgentSession {
    async fn request_identities(&mut self) -> Result<Vec<Identity>, AgentError> {
        let mut identities = vec![];
        for sock_path in &self.socket_paths {
            let mut client = self.connect_upstream_agent(sock_path).await?;
            let agent_identities = client.request_identities().await?;
            {
                let mut known_keys = self.known_keys.lock().expect("Mutex poisoned");
                known_keys.clear();
                for id in &agent_identities {
                    known_keys.insert(id.pubkey.clone(), sock_path.clone());
                }
            }
            identities.extend(agent_identities);
        }

        Ok(identities)
    }

    async fn sign(&mut self, request: SignRequest) -> Result<Signature, AgentError> {
        // Refresh available identities if the public key isn't found
        if !self
            .known_keys
            .lock()
            .expect("Mutex poisoned")
            .contains_key(&request.pubkey)
        {
            let _ = self.request_identities().await?;
        }
        let maybe_agent = self
            .known_keys
            .lock()
            .expect("Mutex poisoned")
            .get(&request.pubkey)
            .cloned();
        if let Some(agent_sock_path) = maybe_agent {
            let mut client = self.connect_upstream_agent(agent_sock_path).await?;
            client.sign(request).await
        } else {
            Err(AgentError::Other(
                format!(
                    "No agent found for public key: {}",
                    request.pubkey.fingerprint(Default::default())
                )
                .into(),
            ))
        }
    }
}

pub struct MuxAgent {
    socket_paths: Vec<PathBuf>,
    known_keys: KnownPubKeys,
}

impl MuxAgent {
    pub async fn run<I, P>(listen_sock: impl AsRef<Path>, agent_socks: I) -> Result<(), AgentError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let socket_paths = agent_socks
            .into_iter()
            .map(|p| p.as_ref().to_path_buf())
            .collect();
        let this = Self {
            socket_paths,
            known_keys: Default::default(),
        };
        this.listen(SelfDeletingUnixListener::bind(listen_sock)?)
            .await?;

        Ok(())
    }
}

impl Agent for MuxAgent {
    #[doc = "Create new session object when a new socket is accepted."]
    fn new_session(&mut self) -> impl Session {
        MuxAgentSession {
            // TODO: should there be a connection pool?
            socket_paths: self.socket_paths.clone(),
            known_keys: Arc::clone(&self.known_keys),
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
        UnixListener::bind(&path).map(|listener| Self { path, listener })
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
        UnixListener::accept(&self.listener)
            .await
            .map(|(s, _addr)| s)
    }
}
