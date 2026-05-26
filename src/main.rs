use std::{path::PathBuf, time::Duration};

use anyhow::Result;
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
        help = "Switch to flash mode before flashing if not already in flash mode"
    )]
    enter_flash_mode: bool,
    #[arg(long, help = "Erase flash sectors occupied by the image")]
    erase: bool,
}

fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args = Ops::parse();
    let mut flash_state = FlashState::from_image(&args.file)?;

    info!("Waiting for device...");

    loop {
        if flash_state.is_done() {
            break;
        }

        let mut dlpc = Dlpc8445Con::wait_for_device()?;

        match run_session(&mut dlpc, &mut flash_state, &args) {
            Err(Dlpc8445Error::UsbDisconnected) => {
                flash_state.reset_current_sector();
                warn!(
                    "DLPC8445 disconnected; resuming from page {} after reconnect",
                    flash_state.current_sector
                );
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(err) => {
                error!("{}", err);
                return Err(err.into());
            }
            _ => {}
        }
    }

    if args.flash {
        info!("Flash programming complete");
    } else {
        info!("Flash validation complete");
    }
    Ok(())
}

fn run_session(
    dlpc: &mut Dlpc8445Con,
    flash_state: &mut FlashState,
    args: &Ops,
) -> std::result::Result<(), Dlpc8445Error> {
    dlpc.verify_flash_mode(args.enter_flash_mode)?;

    let dlpc_info = dlpc.get_info()?;
    if dlpc_info.flash_sector.sector_size as usize != FLASH_SECTOR_SIZE {
        return Err(Dlpc8445Error::general(format!(
            "controller reported an invalid flash sector size of {} bytes, expected {}",
            dlpc_info.flash_sector.sector_size, FLASH_SECTOR_SIZE
        )));
    }

    info!(
        "boot_hold_reason={} flash_id={{manufacturer: 0x{:02X}, device: 0x{:02X}, capacity: 0x{:04X}}} sector_size={} current_sector={}/{}",
        dlpc_info.boot_hold_reason.reason,
        dlpc_info.flash_id.manufacturer,
        dlpc_info.flash_id.device,
        dlpc_info.flash_id.capacity,
        dlpc_info.flash_sector.sector_size,
        flash_state.current_sector,
        flash_state.sectors.len()
    );

    if args.erase {
        dlpc.erase_session(flash_state)
    } else if args.flash {
        dlpc.flash_session(flash_state)
    } else {
        dlpc.validation_session(flash_state)
    }
}
