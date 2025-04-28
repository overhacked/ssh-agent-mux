use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{self, Write},
    time::{Duration, Instant},
};

use duct::{cmd, unix::HandleExt, Handle};
use tempfile::TempPath;

const AGENT_TIMEOUT: Duration = Duration::from_secs(2);
const AGENT_POLL: Duration = Duration::from_micros(100);
const SIGTERM: std::ffi::c_int = 15;

pub enum SshAgentType {
    OpenSsh,
    Mux,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct SshAgentInstance {
    pub handle: Handle,
    pub sock_path: TempPath,
}

fn map_binary_notfound_error(binary_name: &str, err: io::Error) -> io::Error {
    if err.kind() == io::ErrorKind::NotFound {
        io::Error::new(
            err.kind(),
            format!("{binary_name} not found in PATH; OpenSSH client not installed?"),
        )
    } else {
        err
    }
}

impl SshAgentInstance {
    pub fn new<I, A>(agent_type: SshAgentType, args: I) -> io::Result<Self>
    where
        I: IntoIterator<Item = A> + Clone + Send + Sync + 'static,
        A: AsRef<OsStr>,
    {
        let sock_path = tempfile::Builder::new()
            .prefix("agent_")
            .suffix(".sock")
            .tempfile_in(env!("CARGO_TARGET_TMPDIR"))?
            .into_temp_path();
        fs::remove_file(&sock_path)?;

        let cmd = match agent_type {
            SshAgentType::OpenSsh => cmd!("ssh-agent", "-d", "-a", &sock_path),
            SshAgentType::Mux => cmd!(
                env!("CARGO_BIN_EXE_ssh-agent-mux"),
                "--log-level",
                "trace",
                "--listen",
                &sock_path
            ),
        };
        let handle = cmd
            .unchecked()
            .stderr_to_stdout()
            .stdout_capture()
            .before_spawn(move |cmd| {
                let args = args.clone();
                cmd.args(args);
                Ok(())
            })
            .start()?;
        let agent_start_time = Instant::now();
        while !sock_path.exists() {
            std::thread::sleep(AGENT_POLL);
            if agent_start_time.elapsed() >= AGENT_TIMEOUT {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("Couldn't start agent: {:#?}", &handle),
                ));
            }
        }

        Ok(Self { handle, sock_path })
    }

    pub fn new_openssh() -> io::Result<Self> {
        Self::new(SshAgentType::OpenSsh, None::<&OsStr>)
            .map_err(|e| map_binary_notfound_error("ssh-agent", e))
    }

    pub fn new_mux<I, A>(config: &str, args: I) -> io::Result<Self>
    where
        I: IntoIterator<Item = A> + Clone + Send + Sync + 'static,
        A: AsRef<OsStr> + From<OsString> + Clone + Send + Sync + 'static,
    {
        let mut config_file = tempfile::Builder::new()
            .prefix("ssh-mux-agent_")
            .suffix(".toml")
            .tempfile_in(env!("CARGO_TARGET_TMPDIR"))?;
        config_file.write_all(config.as_bytes())?;
        let config_arg: OsString = format!("--config={}", config_file.path().display()).into();
        let mut config_args = vec![A::from(config_arg)];
        config_args.extend(args);

        Self::new(SshAgentType::Mux, config_args)
            .map_err(|e| map_binary_notfound_error(env!("CARGO_BIN_EXE_ssh-agent-mux"), e))
    }

    pub fn add(&self, key: &str) -> io::Result<()> {
        // Add an ssh-key from stdin
        cmd!("ssh-add", "-q", "--", "-")
            .env("SSH_AUTH_SOCK", &self.sock_path)
            .stdin_bytes(key)
            .run()
            .map_err(|e| map_binary_notfound_error("ssh-add", e))?;

        Ok(())
    }

    pub fn list(&self) -> io::Result<Vec<String>> {
        let output = cmd!("ssh-add", "-L")
            .env("SSH_AUTH_SOCK", &self.sock_path)
            .unchecked()
            .stdout_capture()
            .run()
            .map_err(|e| map_binary_notfound_error("ssh-add", e))?;
        let stdout = output.stdout;

        match output.status.code() {
            Some(0) => {
                let lines = String::from_utf8(stdout)
                    .map_err(io::Error::other)?
                    .lines()
                    .map(Into::into)
                    .collect();
                println!(
                    "\nPublic keys in agent ({}):\n{:#?}",
                    self.sock_path.display(),
                    &lines
                );
                Ok(lines)
            }
            Some(1) if stdout.starts_with(b"The agent has no identities.") => Ok(vec![]),
            Some(c) => Err(io::Error::other(format!(
                "command ssh-add -L exited with code {c}; output:\n{}",
                String::from_utf8_lossy(&stdout)
            ))),
            None => Err(io::Error::other(
                "command ssh-add -L exited with an unknown status",
            )),
        }
    }
}

impl Drop for SshAgentInstance {
    fn drop(&mut self) {
        self.handle.send_signal(SIGTERM).expect("SIGTERM failed");
        let output = self.handle.wait().unwrap();
        println!(
            "\nAgent output ({}):\n{}",
            self.sock_path.display(),
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
