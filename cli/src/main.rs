// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use std::io;
use std::io::Write;
use std::path::PathBuf;

use anyhow::bail;
use clap::Parser;
use git_version::git_version;
use log::{info, warn};
use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc::unbounded_channel;

use dlpc8445_proto::flash::FlashState;
use dlpc8445_proto::native::NativeConnection;
use dlpc8445_proto::runner::{Runner, RunnerAction, RunnerCommand};

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Ops {
    #[arg(short, long, default_value = "AWOL_DLP_Upgrade.img")]
    pub file: PathBuf,
    #[arg(long, help = "Program flash (default mode only validates)")]
    pub flash: bool,
    #[arg(
        long,
        help = "Switch to flash mode before flashing if not already in flash mode (DANGER: switching from application mode to Boot ROM invalidates the image on flash; you must flash a valid image immediately afterward)"
    )]
    pub enter_flash_mode: bool,
    #[arg(long, help = "Erase flash sectors occupied by the image")]
    pub erase: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp(None)
        .parse_default_env()
        .init();

    info!(
        "{} - version {} - {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        git_version!()
    );

    let args = Ops::parse();
    let flash_state = FlashState::from_image(&args.file).await?;

    if args.enter_flash_mode {
        confirm_enter_flash_mode().await?;
    }

    info!("Waiting for device...");

    let (event_tx, _event_rx) = unbounded_channel();
    let (command_tx, command_rx) = unbounded_channel();

    let mut runner = Runner::<NativeConnection>::new(event_tx, command_rx);

    command_tx.send(RunnerCommand::LoadImage { image: flash_state })?;
    command_tx.send(RunnerCommand::RequestDeviceAccess)?;

    let action = if args.erase {
        RunnerAction::Erase
    } else if args.flash {
        RunnerAction::Flash
    } else {
        RunnerAction::Verify
    };

    command_tx.send(RunnerCommand::StartAction {
        action,
        enter_flash_mode: args.enter_flash_mode,
    })?;

    // Drop the sender so the command channel closes eventually (if the runner loops again after StartAction)
    drop(command_tx);

    runner.run().await?;

    Ok(())
}

async fn confirm_enter_flash_mode() -> anyhow::Result<()> {
    warn!(
        "WARNING: --enter-flash-mode switches the DLPC 8445 from application mode to Boot ROM, which invalidates the image currently on flash."
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
        bail!("Aborted by user");
    }

    Ok(())
}
