use std::{
    collections::HashMap,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    sync::Arc,
};

use ssh_agent_lib::{
    agent::{self, Agent, ListeningSocket, Session},
    client,
    error::AgentError,
    proto::{Extension, Identity, SignRequest},
};
use ssh_key::{public::KeyData as PubKeyData, Signature};
use tokio::{net::UnixListener, sync::Mutex};

type KnownPubKeys = Arc<Mutex<HashMap<PubKeyData, PathBuf>>>;

struct MuxAgentSession {
    socket_paths: Vec<PathBuf>,
    known_keys: KnownPubKeys,
}

impl MuxAgentSession {
    fn connect_upstream_agent(
        &self,
        sock_path: impl AsRef<Path>,
    ) -> Result<Box<dyn Session>, AgentError> {
        let sock_path = sock_path.as_ref();
        let stream = UnixStream::connect(sock_path)?;
        let client = client::connect(stream.into())
            .map_err(|e| AgentError::Other(format!("Failed to connect to agent: {e}").into()))?;
        log::trace!("Connected to upstream agent on socket: {}", sock_path.display());
        Ok(client)
    }
}

#[ssh_agent_lib::async_trait]
impl Session for MuxAgentSession {
    async fn request_identities(&mut self) -> Result<Vec<Identity>, AgentError> {
        let mut identities = vec![];
        let mut known_keys = self.known_keys.lock().await;
        known_keys.clear();

        for sock_path in &self.socket_paths {
            let mut client = match self.connect_upstream_agent(sock_path) {
                Ok(c) => c,
                // TODO: command-line option to fail on upstream agent failure
                Err(_) => {
                    log::warn!("Ignoring missing upstream agent socket: {}", sock_path.display());
                    continue;
                },
            };
            let agent_identities = client.request_identities().await?;
            {
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
            .await
            .contains_key(&request.pubkey)
        {
            log::debug!("Key not found, re-requesting keys from upstream agents");
            let _ = self.request_identities().await?;
        }
        let maybe_agent = self
            .known_keys
            .lock()
            .await
            .get(&request.pubkey)
            .cloned();
        if let Some(agent_sock_path) = maybe_agent {
            log::info!("Request: signature with key {} from upstream agent <{}>", request.pubkey.fingerprint(Default::default()), agent_sock_path.display());

            let mut client = self.connect_upstream_agent(agent_sock_path)?;
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
            "session-bind@openssh.com" => {
                let mut response = Err(AgentError::ExtensionFailure);
                for sock_path in &self.socket_paths {
                    // Try extension on upstream agents; discard any upstream failures
                    // (but the default is ExtensionFailure if there are no successful
                    // upstream responses)
                    if let Ok(mut client) = self.connect_upstream_agent(sock_path) {
                        if let r @ Ok(_) = client.extension(request.clone()).await {
                            response = r;
                        }
                    }
                }
                response
            },
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
        agent::listen(SelfDeletingUnixListener::bind(listen_sock)?, this)
            .await?;

        Ok(())
    }
}

impl Agent<SelfDeletingUnixListener> for MuxAgent {
    #[doc = "Create new session object when a new socket is accepted."]
    fn new_session(&mut self, _socket: &<SelfDeletingUnixListener as ListeningSocket>::Stream) -> impl Session {
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
