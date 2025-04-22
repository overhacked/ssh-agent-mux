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
    proto::{extension::QueryResponse, Extension, Identity, SignRequest},
    ssh_key::{public::KeyData as PubKeyData, Signature},
};
use tokio::{
    net::UnixListener,
    sync::{Mutex, OwnedMutexGuard},
};

type KnownPubKeysMap = HashMap<PubKeyData, PathBuf>;
type KnownPubKeys = Arc<Mutex<KnownPubKeysMap>>;

/// Only the `request_identities`, `sign`, and `extension` commands are implemented. For
/// `extension`, only the `session-bind@openssh.com` and `query` extensions are supported.
#[ssh_agent_lib::async_trait]
impl Session for MuxAgent {
    async fn request_identities(&mut self) -> Result<Vec<Identity>, AgentError> {
        let mut known_keys = self.known_keys.clone().lock_owned().await;
        self.refresh_identities(&mut known_keys).await
    }

    async fn sign(&mut self, request: SignRequest) -> Result<Signature, AgentError> {
        if let Some(agent_sock_path) = self.get_agent_sock_for_pubkey(&request.pubkey).await? {
            log::info!(
                "Request: signature with key {} from upstream agent <{}>",
                request.pubkey.fingerprint(Default::default()),
                agent_sock_path.display()
            );

            let mut client = self.connect_upstream_agent(agent_sock_path)?;
            client.sign(request).await
        } else {
            let fingerprint = request.pubkey.fingerprint(Default::default());
            log::error!("No upstream agent found for public key {}", &fingerprint);
            log::trace!("Known keys: {:?}", self.known_keys);
            Err(AgentError::Other(
                format!("No agent found for public key: {}", &fingerprint).into(),
            ))
        }
    }

    async fn extension(&mut self, request: Extension) -> Result<Option<Extension>, AgentError> {
        match request.name.as_str() {
            "query" => Ok(Some(Extension::new_message(QueryResponse {
                extensions: ["session-bind@openssh.com"].map(String::from).to_vec(),
            })?)),
            "session-bind@openssh.com" => {
                let mut session_bind_suceeded = false;
                for sock_path in &self.socket_paths {
                    // Try extension on upstream agents; discard any upstream failures from agents
                    // that don't support the extension (but the default is Failure if there are no
                    // successful upstream responses)
                    if let Ok(mut client) = self.connect_upstream_agent(sock_path) {
                        match client.extension(request.clone()).await {
                            // Any agent succeeding is an overall success
                            Ok(v) => {
                                session_bind_suceeded = true;
                                if v.is_some() {
                                    log::warn!("session-bind@openssh.com request succeeded on socket <{}>, but an invalid response was received", sock_path.display());
                                }
                            }
                            // Don't propagate upstream lack of extension support
                            Err(AgentError::Failure) => continue,
                            // Report but ignore any unexpected errors
                            Err(e) => {
                                log::error!("Unexpected error on socket <{}> when requesting session-bind@openssh.com extension: {}", sock_path.display(), e);
                                continue;
                            }
                        }
                    }
                }
                if session_bind_suceeded {
                    Ok(None)
                } else {
                    Err(AgentError::Failure)
                }
            }
            _ => Err(AgentError::Failure),
        }
    }
}

#[derive(Clone)]
pub struct MuxAgent {
    socket_paths: Vec<PathBuf>,
    known_keys: KnownPubKeys,
}

impl MuxAgent {
    /// Run a MuxAgent, listening for SSH agent protocol requests on `listen_sock`, forwarding
    /// requests to the specified paths in `agent_socks`
    pub async fn run<I, P>(listen_sock: impl AsRef<Path>, agent_socks: I) -> Result<(), AgentError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let listen_sock = listen_sock.as_ref();
        let socket_paths: Vec<_> = agent_socks
            .into_iter()
            .map(|p| p.as_ref().to_path_buf())
            .collect();
        if socket_paths.is_empty() {
            log::warn!("Mux agent running but no upstream agents configured");
        }
        log::info!(
            "Starting agent for {} upstream agents; listening on <{}>",
            socket_paths.len(),
            listen_sock.display()
        );
        log::debug!("Upstream agent sockets: {:?}", &socket_paths);

        let listen_sock = match SelfDeletingUnixListener::bind(listen_sock) {
            Ok(s) => s,
            err => {
                log::error!(
                    "Failed to open listening socket at {}",
                    listen_sock.display()
                );
                err?
            }
        };
        let this = Self {
            socket_paths,
            known_keys: Default::default(),
        };
        agent::listen(listen_sock, this).await
    }

    fn connect_upstream_agent(
        &self,
        sock_path: impl AsRef<Path>,
    ) -> Result<Box<dyn Session>, AgentError> {
        let sock_path = sock_path.as_ref();
        let stream = UnixStream::connect(sock_path)?;
        let client = client::connect(stream.into()).map_err(|e| {
            AgentError::Other(
                format!(
                    "Failed to connect to agent at {}: {}",
                    sock_path.display(),
                    e
                )
                .into(),
            )
        })?;
        log::trace!(
            "Connected to upstream agent on socket: {}",
            sock_path.display()
        );
        Ok(client)
    }

    async fn get_agent_sock_for_pubkey(
        &mut self,
        pubkey: &PubKeyData,
    ) -> Result<Option<PathBuf>, AgentError> {
        // Refresh available identities if the public key isn't found;
        // hold lock for duration of signing operation
        let mut known_keys = self.known_keys.clone().lock_owned().await;
        if !known_keys.contains_key(pubkey) {
            log::debug!("Key not found, re-requesting keys from upstream agents");
            let _ = self.refresh_identities(&mut known_keys).await?;
        }
        let maybe_agent = known_keys.get(pubkey).cloned();
        Ok(maybe_agent)
    }

    // Factored out so that the known_keys lock can be held across a total request that includes a
    // refresh of keys from upstream agents
    async fn refresh_identities(
        &mut self,
        known_keys: &mut OwnedMutexGuard<KnownPubKeysMap>,
    ) -> Result<Vec<Identity>, AgentError> {
        let mut identities = vec![];
        known_keys.clear();

        for sock_path in &self.socket_paths {
            let mut client = match self.connect_upstream_agent(sock_path) {
                Ok(c) => c,
                Err(_) => {
                    log::warn!(
                        "Ignoring missing upstream agent socket: {}",
                        sock_path.display()
                    );
                    continue;
                }
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
}

impl Agent<SelfDeletingUnixListener> for MuxAgent {
    #[doc = "Create new session object when a new socket is accepted."]
    fn new_session(
        &mut self,
        _socket: &<SelfDeletingUnixListener as ListeningSocket>::Stream,
    ) -> impl Session {
        self.clone()
    }
}

#[derive(Debug)]
/// A wrapper for UnixListener that keeps the socket path around so it can be deleted
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
