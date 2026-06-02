// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

#[cfg(target_family = "wasm")]
fn main() {
    // nothing to see here
}

#[cfg(not(target_family = "wasm"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use clap::Parser;
    use dlpc8445_proto::{Dlpc8445Error, dlpc8445::Dlpc8445Con, flash::FlashState};
    use git_version::git_version;
    use log::{error, info, warn};

    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    info!(
        "{} - version {} - {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        git_version!()
    );

    let args = native::Ops::parse();
    let mut flash_state = FlashState::from_image(&args.file).await?;

    if args.enter_flash_mode {
        native::confirm_enter_flash_mode().await?;
    }

    info!("Waiting for device...");

    loop {
        let dlpc = {
            use dlpc8445_proto::native::NativeConnection;
            Dlpc8445Con::<NativeConnection>::wait_for_device().await?
        };

        match native::run_session(dlpc, &mut flash_state, &args).await {
            Err(Dlpc8445Error::UsbDisconnected) => {
                warn!("DLPC8445 disconnected");
                flash_state.reset_current_sector();
                info!("Waiting for device to reconnect...");
            }
            Err(err) => {
                error!("{}", err);
                return Err(err.into());
            }
            Ok(msg) => {
                info!("{msg}");
                return Ok(());
            }
        }
    }
}

#[cfg(not(target_family = "wasm"))]
mod native {
    use std::{
        io::{self, Write},
        path::PathBuf,
    };

    use anyhow::{Result, bail};
    use log::{info, warn};

    use dlpc8445_proto::{
        Dlpc8445Error,
        dlpc8445::{Dlpc8445Con, SendCommand},
        flash::{FLASH_SECTOR_SIZE, FlashState},
    };

    #[derive(Debug, clap::Parser)]
    #[command(author, version, about)]
    pub struct Ops {
        #[arg(short, long, default_value = "AWOL_DLP_Upgrade.img")]
        pub file: PathBuf,
        #[arg(long, help = "Program flash (default mode only validates)")]
        pub flash: bool,
        #[arg(
            long,
            help = "Switch to flash mode before flashing if not already in flash mode (DANGER: switching from application to bootrom invalidates the image on flash; you must flash a valid image immediately afterward)"
        )]
        pub enter_flash_mode: bool,
        #[arg(long, help = "Erase flash sectors occupied by the image")]
        pub erase: bool,
    }

    pub async fn confirm_enter_flash_mode() -> Result<()> {
        use tokio::io::AsyncBufReadExt as _;

        warn!(
            "WARNING: --enter-flash-mode switches the DLPC8445 from application mode to bootrom, which invalidates the image currently found on flash."
        );
        warn!(
            "You must flash a valid image immediately after entering flash mode, or the device may not boot."
        );
        warn!("Type yes to continue:");

        print!("> ");
        io::stdout().flush()?;

        let mut response = String::new();
        tokio::io::BufReader::new(tokio::io::stdin())
            .read_line(&mut response)
            .await?;

        let response = response.trim().to_ascii_lowercase();
        if response != "yes" && response != "y" {
            bail!("aborted by user");
        }

        Ok(())
    }

    pub async fn run_session<T: SendCommand>(
        dlpc: Dlpc8445Con<T>,
        flash_state: &mut FlashState,
        args: &Ops,
    ) -> std::result::Result<String, Dlpc8445Error> {
        let mut dlpc = dlpc.verify_flash_mode(args.enter_flash_mode).await?;

        let dlpc_info = dlpc.query_info().await?;
        if dlpc_info.flash_sector.sector_size as usize != FLASH_SECTOR_SIZE {
            return Err(Dlpc8445Error::general(format!(
                "controller reported an invalid flash sector size of {} bytes, expected {}",
                dlpc_info.flash_sector.sector_size, FLASH_SECTOR_SIZE
            )));
        }

        info!(
            "boot_hold_reason={} flash_id={{manufacturer: 0x{:02X}, device: 0x{:02X}, capacity: 0x{:04X}}} sector_size={} current_sector={current}/{total}",
            dlpc_info.boot_hold_reason.reason,
            dlpc_info.flash_id.manufacturer,
            dlpc_info.flash_id.device,
            dlpc_info.flash_id.capacity,
            dlpc_info.flash_sector.sector_size,
            total = flash_state.sectors().len(),
            current = flash_state.current_sector().idx,
        );

        if args.erase {
            dlpc.erase_session(flash_state).await
        } else if args.flash {
            dlpc.flash_session(flash_state).await
        } else {
            dlpc.validation_session(flash_state).await
        }
    }
}
