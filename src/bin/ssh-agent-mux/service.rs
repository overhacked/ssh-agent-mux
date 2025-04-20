use std::{env, io, path::PathBuf};

use clap_serde_derive::clap::{self, Args};
use color_eyre::{eyre::{bail, eyre, Result}, Section};
use service_manager::{ServiceInstallCtx, ServiceManager, ServiceStartCtx, ServiceStatus, ServiceStatusCtx, ServiceStopCtx, ServiceUninstallCtx};

const SERVICE_IDENT: &str = concat!("net.ross-williams.", env!("CARGO_PKG_NAME"));

#[derive(Args, Default)]
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
}

pub fn handle_service_command(args: &ServiceArgs) -> Result<()> {
    let manager = {
        let mut m = <dyn ServiceManager>::native()?;
        if let Err(err) = m.set_level(service_manager::ServiceLevel::User) {
            if err.kind() == io::ErrorKind::Unsupported {
                return handle_set_level_error(args)
            } else {
                Err(err)?
            }
        }
        m
    };

    let label: service_manager::ServiceLabel = SERVICE_IDENT.parse().expect("SERVICE_IDENT is wrong");
    if args.install_service {
        manager.install(ServiceInstallCtx {
            label,
            program: env::current_exe()
                .note(concat!("Could not install service because path to ", env!("CARGO_CRATE_NAME"), " could not be determined."))?,
            args: Vec::default(),
            contents: None,
            username: None,
            working_directory: None,
            environment: None,
            autostart: true,
            disable_restart_on_failure: false,
        })?;
        println!("Installed service {}", SERVICE_IDENT);
    } else if args.restart_service {
        let status = manager.status(ServiceStatusCtx {
            label: label.clone(),
        })?;
        match status {
            ServiceStatus::Running => {
                manager.stop(ServiceStopCtx {
                    label: label.clone(),
                })?;
            },
            ServiceStatus::NotInstalled => {
                bail!("Service {SERVICE_IDENT} not installed; can't restart");
            },
            ServiceStatus::Stopped(_) => (),
        }
        manager.start(ServiceStartCtx {
            label,
        })?;
        println!("Restarted service {}", SERVICE_IDENT);
    } else if args.uninstall_service {
        manager.uninstall(ServiceUninstallCtx {
            label,
        })?;
        println!("Uninstalled service {}", SERVICE_IDENT);
    }

    Ok(())
}

fn handle_set_level_error(args: &ServiceArgs) -> Result<()> {
    let mut err = eyre!("Automatic management of a user service is unsupported on this platform");

    if args.install_service {
        let current_exe = env::current_exe().unwrap_or_else(|_| env!("CARGO_PKG_NAME").into());
        let current_exe_file_name = PathBuf::from(current_exe.file_name().unwrap());
        let arg0 = current_exe_file_name.display();
        err = err.suggestion(format!(r##"
To manually manage starting {arg0}, add the following to your shell startup script:

if ! ps -A -u "$(id -u)" | grep -q {arg0}; then
    {current_exe:?} > /dev/null &
fi
        "##).trim().to_string());
    }

    Err(err)
}
