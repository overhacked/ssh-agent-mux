use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{self, Write},
    time::{Duration, Instant},
};

use duct::{cmd, unix::HandleExt, Handle};
use tempfile::TempPath;

mod keys;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const AGENT_TIMEOUT: Duration = Duration::from_secs(2);
const AGENT_POLL: Duration = Duration::from_micros(100);
const SIGTERM: std::ffi::c_int = 15;

enum SshAgentType {
    OpenSsh,
    Mux,
}

#[allow(dead_code)]
#[derive(Debug)]
struct SshAgentInstance {
    handle: Handle,
    sock_path: TempPath,
}

impl SshAgentInstance {
    fn new<I, A>(agent_type: SshAgentType, args: I) -> io::Result<Self>
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
                "debug",
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

    fn new_openssh() -> io::Result<Self> {
        Self::new(SshAgentType::OpenSsh, None::<&OsStr>)
    }

    fn new_mux<I, A>(config: &str, args: I) -> io::Result<Self>
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
    }

    fn add(&self, key: &str) -> io::Result<()> {
        // Add an ssh-key from stdin
        cmd!("ssh-add", "-q", "--", "-")
            .env("SSH_AUTH_SOCK", &self.sock_path)
            .stdin_bytes(key)
            .run()?;

        Ok(())
    }

    fn list(&self) -> io::Result<Vec<String>> {
        let output = cmd!("ssh-add", "-L")
            .env("SSH_AUTH_SOCK", &self.sock_path)
            .unchecked()
            .stdout_capture()
            .run()
            .and_then(|o| match o.status.code() {
                Some(0) => Ok(o.stdout),
                Some(1) if o.stdout.starts_with(b"The agent has no identities.") => Ok(vec![]),
                Some(c) => Err(io::Error::other(format!(
                    "command ssh-add -L exited with code {}; output:\n{}",
                    c,
                    String::from_utf8_lossy(&o.stdout)
                ))),
                None => Err(io::Error::other(
                    "command ssh-add -L exited with an unknown status",
                )),
            })
            .and_then(|s| String::from_utf8(s).map_err(io::Error::other))?;
        let lines = output.lines().map(Into::into).collect();
        println!(
            "\nPublic keys in agent ({}):\n{:?}",
            self.sock_path.display(),
            &lines
        );
        Ok(lines)
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

fn make_openssh_agent_with_keys() -> io::Result<SshAgentInstance> {
    let agent = SshAgentInstance::new_openssh()?;
    println!("{:#?}", agent);

    for key in keys::PRIVATE {
        agent.add(key)?;
    }

    Ok(agent)
}

fn assert_all_keys_in_agent(agent: &SshAgentInstance) -> TestResult {
    let keys_in_agent = agent.list()?;
    for key in keys::PUBLIC {
        assert!(keys_in_agent.iter().any(|v| v == key));
    }

    Ok(())
}

#[test]
fn add_keys_to_openssh_agent() -> TestResult {
    let agent = make_openssh_agent_with_keys()?;

    assert_all_keys_in_agent(&agent)?;

    Ok(())
}

#[test]
fn empty_mux_agent() -> TestResult {
    let agent = SshAgentInstance::new_mux("", None::<OsString>)?;

    let keys_in_agent = agent.list()?;
    assert!(keys_in_agent.is_empty());

    Ok(())
}

#[test]
fn mux_with_one_agent() -> TestResult {
    let openssh_agent = make_openssh_agent_with_keys()?;
    let mux_agent = SshAgentInstance::new_mux(
        &format!(
            r##"agent_sock_paths = ["{}"]"##,
            openssh_agent.sock_path.display()
        ),
        None::<OsString>,
    )?;

    assert_all_keys_in_agent(&mux_agent)?;

    Ok(())
}

#[test]
fn mux_with_three_agents() -> TestResult {
    let agent_rsa = SshAgentInstance::new_openssh()?;
    agent_rsa.add(keys::TEST_KEY_RSA)?;
    let agent_ecdsa = SshAgentInstance::new_openssh()?;
    agent_ecdsa.add(keys::TEST_KEY_ECDSA)?;
    let agent_ed25519 = SshAgentInstance::new_openssh()?;
    agent_ed25519.add(keys::TEST_KEY_ED25519)?;

    let mux_agent = SshAgentInstance::new_mux(
        &format!(
            r##"agent_sock_paths = ["{}", "{}", "{}"]"##,
            dbg!(&agent_rsa).sock_path.display(),
            dbg!(&agent_ecdsa).sock_path.display(),
            dbg!(&agent_ed25519).sock_path.display()
        ),
        None::<OsString>,
    )?;

    assert_all_keys_in_agent(dbg!(&mux_agent))?;

    Ok(())
}
