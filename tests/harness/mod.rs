#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{self, Write},
    path::PathBuf,
    time::{Duration, Instant},
};

use duct::{cmd, unix::HandleExt, Handle};
use tempfile::TempPath;

const CRATE_MAIN_BIN: &str = env!(concat!("CARGO_BIN_EXE_", env!("CARGO_PKG_NAME")));
const AGENT_TIMEOUT: Duration = Duration::from_secs(2);
const AGENT_POLL: Duration = Duration::from_micros(100);
const SIGTERM: std::ffi::c_int = 15;
const SSH_KEY_FILE_MODE: u32 = 0o400;

#[derive(Debug)]
pub struct SshPkiKeygen {
    pub ca_key: Vec<u8>,
    pub serial: usize,
}

impl SshPkiKeygen {
    const CA_KEY_FILENAME: &str = "ca_key";
    const USER_KEY_FILENAME: &str = "user_key";

    pub fn new() -> io::Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        cmd!("ssh-keygen", "-q", "-N", "", "-f", Self::CA_KEY_FILENAME)
            .dir(temp_dir.path())
            .run()
            .map_err(|e| map_binary_notfound_error("ssh-keygen", e))?;

        let ca_key = fs::read(temp_dir.path().join(Self::CA_KEY_FILENAME))?;

        Ok(Self { ca_key, serial: 0 })
    }

    fn make_ca_dir(&self) -> io::Result<(tempfile::TempDir, PathBuf)> {
        let temp_dir = tempfile::tempdir()?;
        let ca_key_path = temp_dir.path().join(Self::CA_KEY_FILENAME);
        fs::write(&ca_key_path, &self.ca_key)?;
        #[cfg(unix)]
        fs::set_permissions(&ca_key_path, fs::Permissions::from_mode(SSH_KEY_FILE_MODE))?;
        Ok((temp_dir, ca_key_path))
    }

    pub fn sign(&mut self, key: impl AsRef<[u8]>) -> io::Result<Vec<u8>> {
        let key = key.as_ref();

        let (ca_dir, ca_key_path) = self.make_ca_dir()?;

        let user_key_path = ca_dir.path().join(Self::USER_KEY_FILENAME);
        fs::write(&user_key_path, key)?;

        self.serial += 1;
        #[rustfmt::skip]
        cmd!(
            "ssh-keygen",
            "-q",
            "-s", &ca_key_path,
            "-I", "test_user",
            "-n", "test_user",
            "-V", "+1h",
            "-z", self.serial.to_string(),
            &user_key_path
        )
        .dir(ca_dir.path())
        .run()
        .map_err(|e| map_binary_notfound_error("ssh-keygen", e))?;

        let user_cert_path = {
            // Get the filename part of the key path
            let mut user_cert_filename = user_key_path
                .file_name()
                .map(|s| s.to_os_string())
                .unwrap_or_default();
            user_cert_filename.push("-cert.pub");
            // Clone the key path, but replace the filename with the `-cert.pub`-appended filename
            let mut user_cert_path = user_key_path.clone();
            user_cert_path.set_file_name(user_cert_filename);
            user_cert_path
        };

        fs::read(&user_cert_path)
    }
}

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
                CRATE_MAIN_BIN,
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
            .map_err(|e| map_binary_notfound_error(CRATE_MAIN_BIN, e))
    }

    pub fn add(&self, key: impl AsRef<[u8]>) -> io::Result<()> {
        // Add an ssh-key from stdin
        cmd!("ssh-add", "-q", "--", "-")
            .env("SSH_AUTH_SOCK", &self.sock_path)
            .stdin_bytes(key.as_ref())
            .run()
            .map_err(|e| map_binary_notfound_error("ssh-add", e))?;

        Ok(())
    }

    pub fn add_cert(&self, key: impl AsRef<[u8]>, cert: impl AsRef<[u8]>) -> io::Result<()> {
        // Add a key and certificate pair from temporary files
        let temp_dir = tempfile::tempdir()?;
        let key_file = temp_dir.path().join("key");
        fs::write(&key_file, key.as_ref())?;
        #[cfg(unix)]
        fs::set_permissions(&key_file, fs::Permissions::from_mode(SSH_KEY_FILE_MODE))?;
        let cert_file = temp_dir.path().join("key-cert.pub");
        fs::write(&cert_file, cert.as_ref())?;

        cmd!("ssh-add", "-q", "--", &key_file)
            .env("SSH_AUTH_SOCK", &self.sock_path)
            .run()
            .map_err(|e| map_binary_notfound_error("ssh-add", e))?;

        Ok(())
    }

    pub fn list(&self) -> io::Result<Vec<String>> {
        let output = cmd!("ssh-add", "-L")
            .env("SSH_AUTH_SOCK", &self.sock_path)
            .unchecked()
            .stdout_capture()
            .stderr_capture()
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
            other_code => {
                println!(
                    "`ssh-add -L` output:\n{}{}{}{}",
                    String::from_utf8_lossy(&stdout),
                    if !stdout.is_empty() { "\n" } else { "" },
                    String::from_utf8_lossy(&output.stderr),
                    if !output.stderr.is_empty() { "\n" } else { "" }
                );
                Err(io::Error::other(format!(
                    "command ssh-add -L exited with {}",
                    other_code.map_or_else(
                        || String::from("an unknown status"),
                        |c| format!("code {c}"),
                    )
                )))
            }
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
