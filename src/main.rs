use std::{
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Result, bail};
use clap::Parser;
use log::{error, info, warn};

use dlpc8445_proto::{
    Dlpc8445Error,
    dlpc8445::Dlpc8445Con,
    flash::{FLASH_SECTOR_SIZE, FlashState},
};

#[derive(Debug, clap::Parser)]
#[command(author, version, about)]
struct Ops {
    #[arg(short, long, default_value = "AWOL_DLP_Upgrade.img")]
    file: PathBuf,
    #[arg(long, help = "Program flash (default mode only validates)")]
    flash: bool,
    #[arg(
        long,
        help = "Switch to flash mode before flashing if not already in flash mode (DANGER: switching from application to bootrom invalidates the image on flash; you must flash a valid image immediately afterward)"
    )]
    enter_flash_mode: bool,
    #[arg(long, help = "Erase flash sectors occupied by the image")]
    erase: bool,
}

fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let args = Ops::parse();
    let mut flash_state = FlashState::from_image(&args.file)?;

    if args.enter_flash_mode {
        confirm_enter_flash_mode()?;
    }

    info!("Waiting for device...");

    loop {
        let mut dlpc = Dlpc8445Con::wait_for_device()?;

        match run_session(&mut dlpc, &mut flash_state, &args) {
            Err(Dlpc8445Error::UsbDisconnected) => {
                warn!("DLPC8445 disconnected");
                flash_state.reset_current_sector();
                std::thread::sleep(Duration::from_millis(250));
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

fn confirm_enter_flash_mode() -> Result<()> {
    warn!(
        "WARNING: --enter-flash-mode switches the DLPC8445 from application mode to bootrom, \
which invalidates the image currently found on flash."
    );
    warn!(
        "You must flash a valid image immediately after entering flash mode, or the device may not boot."
    );
    warn!("Type yes to continue:");

    print!("> ");
    io::stdout().flush()?;

    let mut response = String::new();
    io::stdin().read_line(&mut response)?;

    let response = response.trim().to_ascii_lowercase();
    if response != "yes" && response != "y" {
        bail!("aborted by user");
    }

    Ok(())
}

fn run_session(
    dlpc: &mut Dlpc8445Con,
    flash_state: &mut FlashState,
    args: &Ops,
) -> std::result::Result<String, Dlpc8445Error> {
    dlpc.verify_flash_mode(args.enter_flash_mode)?;

    let dlpc_info = dlpc.query_info()?;
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
        dlpc.erase_session(flash_state)
    } else if args.flash {
        dlpc.flash_session(flash_state)
    } else {
        dlpc.validation_session(flash_state)
    }
}
