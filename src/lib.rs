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
    proto::{Extension, Identity, SignRequest},
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
        let sock_path = sock_path.as_ref();
        let stream = UnixStream::connect(sock_path)?;
        let client = connect(stream.into())
            .await
            .map_err(|e| AgentError::Other(format!("Failed to connect to agent: {e}").into()))?;
        log::trace!("Connected to upstream agent on socket: {}", sock_path.display());
        Ok(client)
    }
}

#[ssh_agent_lib::async_trait]
impl Session for MuxAgentSession {
    async fn request_identities(&mut self) -> Result<Vec<Identity>, AgentError> {
        let mut identities = vec![];
        self.known_keys.lock().expect("Mutex poisoned").clear();

        for sock_path in &self.socket_paths {
            let mut client = match self.connect_upstream_agent(sock_path).await {
                Ok(c) => c,
                // TODO: command-line option to fail on upstream agent failure
                Err(_) => {
                    log::warn!("Ignoring missing upstream agent socket: {}", sock_path.display());
                    continue;
                },
            };
            let agent_identities = client.request_identities().await?;
            {
                let mut known_keys = self.known_keys.lock().expect("Mutex poisoned");
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
            log::debug!("Key not found, re-requesting keys from upstream agents");
            let _ = self.request_identities().await?;
        }
        let maybe_agent = self
            .known_keys
            .lock()
            .expect("Mutex poisoned")
            .get(&request.pubkey)
            .cloned();
        if let Some(agent_sock_path) = maybe_agent {
            log::info!("Request: signature with key {} from upstream agent <{}>", request.pubkey.fingerprint(Default::default()), agent_sock_path.display());

            let mut client = self.connect_upstream_agent(agent_sock_path).await?;
            client.sign(request).await
        } else {
            let fingerprint = request.pubkey.fingerprint(Default::default());
            log::error!("No upstream agent found for public key {}", &fingerprint);
            log::trace!("Known keys: {:?}", self.known_keys);
            Err(AgentError::Other(
                format!(
                    "No agent found for public key: {}",
                    &fingerprint
                )
                .into(),
            ))
        }
    }

    async fn extension(&mut self, request: Extension) -> Result<Option<Extension>, AgentError> {
        match request.name.as_str() {
            "query" => Ok(Some(Extension { name: request.name, details: Vec::default().into(), })),
            _ => Err(AgentError::ExtensionFailure),
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
        let socket_paths: Vec<_> = agent_socks
            .into_iter()
            .map(|p| p.as_ref().to_path_buf())
            .collect();
        log::info!("Starting agent for {} upstream agents; listening on <{}>", socket_paths.len(), listen_sock.as_ref().display());
        log::debug!("Upstream agent sockets: {:?}", &socket_paths);
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
