use std::{ffi::OsString, io};

use duct::cmd;
use harness::SshAgentInstance;

mod harness;
mod keys;

type TestResult = Result<(), Box<dyn std::error::Error>>;

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

/// Regression test for issue #90: mux agent must handle
/// SSH_AGENTC_ADD_ID_CONSTRAINED (command 25), which is sent by ssh-add when
/// the -t flag (lifetime) or -c flag (confirm) is used.
///
/// The mux proxies the constrained-add to all upstream agents; the key must
/// then appear when listing through the mux.
#[test]
fn add_constrained_key_to_mux() -> TestResult {
    let tmpdir = tempfile::TempDir::new()?;
    let key_path = tmpdir.path().join("test_key");

    cmd!("ssh-keygen", "-t", "ed25519", "-f", &key_path, "-N", "")
        .stdout_null()
        .stderr_null()
        .run()
        .map_err(io::Error::other)?;

    // A real upstream agent is required: the mux has no local key storage and
    // forwards add operations to its upstreams.
    let upstream = SshAgentInstance::new_openssh()?;

    let mux_agent = SshAgentInstance::new_mux(
        &format!(
            r##"agent_sock_paths = ["{}"]"##,
            upstream.sock_path.display()
        ),
        None::<OsString>,
    )?;

    // -t 3600 triggers SSH_AGENTC_ADD_ID_CONSTRAINED (command 25) instead of
    // the plain SSH_AGENTC_ADD_IDENTITY (command 17).
    mux_agent.add_timed(&key_path, 3600)?;

    // The constrained key must be visible when listing through the mux.
    let keys = mux_agent.list()?;
    assert!(
        !keys.is_empty(),
        "constrained key was not added to the mux agent"
    );

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
