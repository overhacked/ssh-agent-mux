use std::{
    fs,
    io::{self, Write},
    process::Output,
};

use duct::cmd;
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn make_fake_home() -> Result<TempDir, io::Error> {
    let fake_home = tempfile::Builder::new()
        .prefix("fake_home_")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))?;
    Ok(fake_home)
}

fn run_in_fake_home(
    home: &TempDir,
    args: &'static [&str],
) -> Result<Output, Box<dyn std::error::Error>> {
    let cmd_result = cmd!(env!("CARGO_BIN_EXE_ssh-agent-mux"), "--log-level", "trace")
        .before_spawn(move |cmd| {
            cmd.args(args);
            Ok(())
        })
        // Make sure to keep these environment variables in sync
        // with the logic of ssh_agent_mux::cli::default_config_path()
        .env("HOME", home.path())
        .env_remove("XDG_CONFIG_HOME")
        .stdout_capture()
        .stderr_capture()
        .unchecked()
        .run()?;

    Ok(cmd_result)
}

fn dump_command_output(output: &Output) -> Result<(), Box<dyn std::error::Error>> {
    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;
    Ok(())
}

#[test]
fn install_config() -> TestResult {
    let temp_home = make_fake_home()?;
    let temp_xdg_config_home = temp_home.path().join(".config");
    fs::create_dir(&temp_xdg_config_home)?;

    let output = run_in_fake_home(&temp_home, &["--install-config"])?;
    if !output.status.success() {
        dump_command_output(&output)?;
        Err("Failed to install configuration file")?;
    }

    let expected_config_file = temp_xdg_config_home
        .join(env!("CARGO_PKG_NAME"))
        .join(concat!(env!("CARGO_PKG_NAME"), ".toml"));
    if !expected_config_file.is_file() {
        dump_command_output(&output)?;
        let _ = temp_home.keep();
        Err(format!(
            "`{} --install-config` reported success, but config file does not exist ({})",
            env!("CARGO_PKG_NAME"),
            expected_config_file.display()
        ))?;
    }

    Ok(())
}

#[test]
fn missing_dot_config_dir() -> TestResult {
    let temp_home = make_fake_home()?;

    let output = run_in_fake_home(&temp_home, &["--install-config"])?;
    if output.status.success() {
        Err(format!(
            "Failed to detect missing .config directory in {}",
            temp_home.path().display()
        ))?;
    }
    if !String::from_utf8_lossy(&output.stderr).contains("parent directory does not exist") {
        dump_command_output(&output)?;
        Err("Expected error output not found")?;
    }

    Ok(())
}
