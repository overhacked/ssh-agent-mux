use std::{env, fmt::Write, fs, io, path::PathBuf};

use clap_serde_derive::clap::{self, Args};
use color_eyre::{
    eyre::{bail, eyre, Result},
    Section,
};
use service_manager::{
    ServiceInstallCtx, ServiceManager, ServiceStartCtx, ServiceStatus, ServiceStatusCtx,
    ServiceStopCtx, ServiceUninstallCtx,
};

use crate::cli::Config;

const SERVICE_IDENT: &str = concat!("net.ross-williams.", env!("CARGO_PKG_NAME"));

#[derive(Args, Clone, Copy, Default)]
#[group(multiple = false)]
pub struct ServiceArgs {
    /// Install the user service manager configuration
    #[arg(long)]
    pub install_service: bool,

    /// Start the service if it is not started
    #[arg(long)]
    pub restart_service: bool,

    /// Uninstall the user service manager configuration
    #[arg(long)]
    pub uninstall_service: bool,

    /// Install the user service manager configuration
    #[arg(long)]
    pub install_config: bool,
}

impl ServiceArgs {
    // Return `true` if any of the service-related args have been supplied
    pub fn any(&self) -> bool {
        self.install_service
            || self.restart_service
            || self.uninstall_service
            || self.install_config
    }
}

pub fn handle_service_command(config: &Config) -> Result<()> {
    if config.service.install_config {
        if !config.config_path.try_exists()? {
            return write_new_config_file(config);
        } else {
            bail!("Config file at {} already exists. Delete it and run --install-config again if you want to re-generate", config.config_path.display());
        }
    }

    let manager = {
        let mut m = <dyn ServiceManager>::native()?;
        if let Err(err) = m.set_level(service_manager::ServiceLevel::User) {
            if err.kind() == io::ErrorKind::Unsupported {
                return handle_set_level_error(&config.service);
            } else {
                Err(err)?
            }
        }
        m
    };

    let label: service_manager::ServiceLabel =
        SERVICE_IDENT.parse().expect("SERVICE_IDENT is wrong");
    if config.service.install_service {
        if !config.config_path.try_exists()? {
            write_new_config_file(config)?;
        }
        manager.install(ServiceInstallCtx {
            label,
            program: env::current_exe().note(concat!(
                "Could not install service because path to ",
                env!("CARGO_CRATE_NAME"),
                " could not be determined."
            ))?,
            args: Vec::default(),
            contents: None,
            username: None,
            working_directory: None,
            environment: None,
            autostart: true,
            disable_restart_on_failure: false,
        })?;
        println!("Installed service {}", SERVICE_IDENT);
    } else if config.service.restart_service {
        let status = manager.status(ServiceStatusCtx {
            label: label.clone(),
        })?;
        match status {
            ServiceStatus::Running => {
                manager.stop(ServiceStopCtx {
                    label: label.clone(),
                })?;
            }
            ServiceStatus::NotInstalled => {
                bail!("Service {SERVICE_IDENT} not installed; can't restart");
            }
            ServiceStatus::Stopped(_) => (),
        }
        manager.start(ServiceStartCtx { label })?;
        println!("Restarted service {}", SERVICE_IDENT);
    } else if config.service.uninstall_service {
        manager.uninstall(ServiceUninstallCtx { label })?;
        println!("Uninstalled service {}", SERVICE_IDENT);
    }

    Ok(())
}

fn write_new_config_file(config: &Config) -> Result<()> {
    let mut success_msg = format!(
        "Automatically creating configuration file at {} ",
        config.config_path.display()
    );

    let mut new_config = config.clone();
    if config.agent_sock_paths.is_empty() {
        match env::var("SSH_AUTH_SOCK") {
            Ok(v) => {
                success_msg.write_str("with the current SSH_AUTh_SOCK as the upstream agent; please edit to add additional agents.")?;
                new_config.agent_sock_paths.push(v.into());
            }
            Err(e) => {
                let mut emsg = String::from("A new configuration file cannot be created: ");
                match e {
                    env::VarError::NotPresent => {
                        emsg.write_str("SSH_AUTH_SOCK is not in the environment, and no upstream agent paths were specified on the command line.")?;
                    }
                    env::VarError::NotUnicode(_) => {
                        emsg.write_str(
                            "SSH_AUTH_SOCK is defined, but contains non-UTF-8 characters.",
                        )?;
                    }
                }
                bail!(emsg);
            }
        };
    } else {
        write!(
            success_msg,
            "with the upstream agent socket paths specified on the command line."
        )?;
    }

    println!("{}", success_msg);
    let new_config_toml = toml::to_string_pretty(&new_config)?;
    fs::write(&config.config_path, new_config_toml.as_bytes())?;
    Ok(())
}

fn handle_set_level_error(args: &ServiceArgs) -> Result<()> {
    let mut err = eyre!("Automatic management of a user service is unsupported on this platform");

    if args.install_service {
        let current_exe = env::current_exe().unwrap_or_else(|_| env!("CARGO_PKG_NAME").into());
        let current_exe_file_name = PathBuf::from(current_exe.file_name().unwrap());
        let arg0 = current_exe_file_name.display();
        err = err.suggestion(format!(
            r##"
To manually manage starting {arg0}, add the following to your shell startup script:

if ! ps -A -u "$(id -u)" | grep -q {arg0}; then
    {current_exe:?} > /dev/null &
fi"##
        ));
    }

    Err(err)
}
