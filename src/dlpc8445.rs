use std::{
    io::{Read, Write},
    time::Duration,
};

use log::{error, info, trace, warn};
use nusb::{
    MaybeFuture,
    io::{EndpointRead, EndpointWrite},
    transfer::{Bulk, In, Out},
};

use crate::{
    Dlpc8445Error, Result,
    flash::{FLASH_PAGE_SIZE, FlashSector},
    protocol::{
        ApplicationMode, BootHoldReasonResponse, FlashIdResponse, FlashSectorInformationResponse,
        ReadBootHoldReasonCommand, ReadFlashIdCommand, ReadGetFlashSectorInformationCommand,
        ReadModeCommand, SwitchApplicationOption, WriteSwitchApplicationCommand,
    },
};
use crate::{
    flash::{FLASH_PAGE_PROGRAM_TIME, FLASH_SECTOR_ERASE_TIME, FlashState},
    protocol::{
        ChecksumResponse, Command, FlashWriteCommand, ReadChecksumCommand,
        ReadUnlockFlashForUpdateCommand, ResponsePacket, ResponsePayload, WriteEraseSectorCommand,
        WriteInitializeFlashReadWriteSettingsCommand, WriteUnlockFlashForUpdateCommand,
    },
};

const VENDOR_ID: u16 = 0x0451;
const PRODUCT_ID: u16 = 0x8430;
const MAX_PAGE_REPROGRAM_ATTEMPTS: usize = 3;

pub struct Dlpc8445Con {
    writer: EndpointWrite<Bulk>,
    reader: EndpointRead<Bulk>,
    info: Option<Dlpc8445Info>,
}

pub struct Dlpc8445Info {
    pub boot_hold_reason: BootHoldReasonResponse,
    pub flash_id: FlashIdResponse,
    pub flash_sector: FlashSectorInformationResponse,
    pub mode: ApplicationMode,
}

impl Dlpc8445Con {
    pub fn wait_for_device() -> Result<Self> {
        let di = loop {
            let device = nusb::list_devices()
                .wait()?
                .find(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID);

            if let Some(device) = device {
                info!("DLPC8445 device found");
                break device;
            }

            std::thread::sleep(Duration::from_millis(100));
        };

        let device = di.open().wait()?;
        let interface = device.claim_interface(0).wait()?;

        let writer = interface
            .endpoint::<Bulk, Out>(0x01)?
            .writer(512)
            .with_write_timeout(Duration::from_millis(500));

        let reader = interface
            .endpoint::<Bulk, In>(0x81)?
            .reader(512)
            .with_read_timeout(Duration::from_secs(1));

        Ok(Self {
            writer,
            reader,
            info: None,
        })
    }

    pub fn query_info(&mut self) -> Result<&Dlpc8445Info> {
        let boot_hold_reason = self.send_command(ReadBootHoldReasonCommand)?;
        let flash_info = self.send_command(ReadFlashIdCommand)?;
        let flash_sector_info = self.send_command(ReadGetFlashSectorInformationCommand)?;
        let mode = self.send_command(ReadModeCommand)?.application_mode();

        self.info = Some(Dlpc8445Info {
            boot_hold_reason,
            flash_id: flash_info,
            flash_sector: flash_sector_info,
            mode,
        });

        self.info
            .as_ref()
            .ok_or_else(|| Dlpc8445Error::general("failed to query device info"))
    }

    pub fn send_command<T, R>(&mut self, command: T) -> Result<R>
    where
        T: Command<ResponsePacket<R> = ResponsePacket<R>>,
        R: ResponsePayload,
    {
        trace!("Sending command: {:?}", command);
        // Bug in boot rom, no response if checksum is present!
        let checksum_present = self.info.as_ref().is_some_and(|info| {
            matches!(
                info.mode,
                ApplicationMode::MainApplication | ApplicationMode::SecondaryBootApplication
            )
        });
        let command = command
            .into_packet()?
            .set_checksum_present(checksum_present);
        let encoded = command.encode()?;

        self.writer.write_all(&encoded)?;
        self.writer.flush_end()?;

        let mut response = Vec::new();
        let mut reader = self.reader.until_short_packet();
        reader.read_to_end(&mut response)?;
        reader
            .consume_end()
            .map_err(|err| Dlpc8445Error::general(err.to_string()))?;

        command
            .decode::<R>(&response)
            .and_then(|resp| R::decode(resp.data))
    }

    pub fn flash_session(&mut self, flash_state: &mut FlashState) -> Result<()> {
        self.unlock_flash()?;

        if flash_state.header_sector_needs_invalidation() {
            info!("Erasing first sector on flash to invalidate image");
            let header = flash_state.header_sector();
            self.erase_sector(header)?;
            info!(
                "Flashing sectors in reverse (from last to first) to ensure boot rom fallback for partial flashed images"
            );
            flash_state.reverse();
        }

        while !flash_state.is_done() {
            let sector = flash_state.current_sector();

            if self.validate_sector(sector).is_ok() {
                info!(
                    "Sector {} at 0x{:08X} already matches image; skipping",
                    sector.idx, sector.start_addr
                );

                flash_state.advance_sector();
                continue;
            }

            info!(
                "Sector {} checksum mismatch; erasing and programming",
                sector.idx,
            );

            let mut reprogram_attempts = 0usize;

            while !sector.verified {
                if !sector.erased {
                    info!(
                        "Erasing sector {} at 0x{:08X}",
                        sector.idx, sector.start_addr
                    );
                    self.erase_sector(sector)?;
                }

                if !sector.is_programmed() {
                    info!(
                        "Programming sector {} at 0x{:08X}",
                        sector.idx, sector.start_addr,
                    );
                    self.program_sector(sector)?;
                }

                info!(
                    "Validating sector {} at 0x{:08X}-0x{:08X}",
                    sector.idx, sector.start_addr, sector.end_addr
                );

                if let Err(err) = self.validate_sector(sector) {
                    if reprogram_attempts >= MAX_PAGE_REPROGRAM_ATTEMPTS {
                        return Err(Dlpc8445Error::general(format!(
                            "validation failed for sector {} after {} reprogram attempts: {}",
                            sector.idx, MAX_PAGE_REPROGRAM_ATTEMPTS, err
                        )));
                    }

                    reprogram_attempts += 1;
                    warn!(
                        "Validation failed for sector {}; erasing and reprogramming (attempt {}/{})",
                        sector.idx, reprogram_attempts, MAX_PAGE_REPROGRAM_ATTEMPTS
                    );
                    sector.reset();
                }
            }

            info!("Sector {} complete", sector.idx);
            flash_state.advance_sector();
        }

        self.send_command(WriteUnlockFlashForUpdateCommand::lock())?;
        Ok(())
    }

    pub fn validation_session(&mut self, flash_state: &mut FlashState) -> Result<()> {
        while !flash_state.is_done() {
            let sector = flash_state.current_sector();

            match self.validate_sector(sector) {
                Ok(_) => info!("Sector {}: valid", sector.idx),
                Err(err) => error!("Sector {}: invalid {err}", sector.idx),
            }
            flash_state.advance_sector();
        }

        let (total, valid, invalid) = flash_state.validation_result();

        let msg = format!(
            "Validation complete: {valid}/{total} sectors valid, {invalid} sectors invalid"
        );

        if total == valid {
            info!("{msg}");
            return Ok(());
        } else {
            Err(Dlpc8445Error::general(msg))
        }
    }

    fn unlock_flash(&mut self) -> Result<()> {
        self.send_command(WriteUnlockFlashForUpdateCommand::unlock())?;
        let unlock_state = self.send_command(ReadUnlockFlashForUpdateCommand)?;

        if !unlock_state.is_unlocked() {
            return Err(Dlpc8445Error::general(
                "failed to unlock flash update commands",
            ));
        }
        Ok(())
    }

    fn initialize_flash_rw(&mut self, start_address: usize, num_bytes: usize) -> Result<()> {
        let command = WriteInitializeFlashReadWriteSettingsCommand {
            start_address: start_address.try_into().map_err(|_| {
                Dlpc8445Error::general(format!(
                    "flash start address 0x{start_address:08X} does not fit into u32"
                ))
            })?,
            num_bytes: num_bytes.try_into().map_err(|_| {
                Dlpc8445Error::general(format!(
                    "flash byte count {num_bytes} does not fit into u32"
                ))
            })?,
        };

        self.send_command(command)?;
        Ok(())
    }

    fn program_sector(&mut self, sector: &mut FlashSector) -> Result<()> {
        if sector.remaining() == 0 {
            return Ok(());
        }

        self.initialize_flash_rw(sector.start_addr + sector.current_addr, sector.remaining())?;

        while sector.current_addr < sector.len() {
            let next_pos = (sector.current_addr + FLASH_PAGE_SIZE).min(sector.len());
            let chunk = &sector.data[sector.current_addr..next_pos];

            self.send_command(FlashWriteCommand {
                data: chunk.to_vec(),
            })?;
            std::thread::sleep(FLASH_PAGE_PROGRAM_TIME);
            sector.current_addr = next_pos;
        }

        Ok(())
    }

    fn erase_sector(&mut self, sector: &mut FlashSector) -> Result<()> {
        let sector_address = sector.start_addr.try_into().map_err(|_| {
            Dlpc8445Error::general(format!(
                "flash sector start address 0x{:08X} does not fit into u32",
                sector.start_addr
            ))
        })?;
        self.send_command(WriteEraseSectorCommand::new(sector_address))?;
        std::thread::sleep(FLASH_SECTOR_ERASE_TIME);
        sector.mark_erased();
        Ok(())
    }

    fn validate_sector(&mut self, sector: &mut FlashSector) -> Result<()> {
        let image_checksum = sector.checksum;
        let flash_checksum = self.read_sector_checksum(sector)?.as_u64();

        if flash_checksum != image_checksum {
            return Err(Dlpc8445Error::general(format!(
                "verification failed for sector {} at 0x{:08X}: checksum mismatch (image=0x{:08X}, flash=0x{:08X})",
                sector.idx, sector.start_addr, image_checksum, flash_checksum
            )));
        }

        sector.verified = true;

        Ok(())
    }

    fn read_sector_checksum(&mut self, sector: &FlashSector) -> Result<ChecksumResponse> {
        let start_address = sector.start_addr.try_into().map_err(|_| {
            Dlpc8445Error::general(format!(
                "flash start address 0x{:08X} does not fit into u32",
                sector.start_addr
            ))
        })?;
        let num_bytes = sector.len().try_into().map_err(|_| {
            Dlpc8445Error::general(format!(
                "flash byte count {} does not fit into u32",
                sector.len()
            ))
        })?;

        self.send_command(ReadChecksumCommand {
            start_address,
            num_bytes,
        })
    }

    pub fn verify_flash_mode(&mut self, enter_flash_mode: bool) -> Result<()> {
        let current_mode = self.send_command(ReadModeCommand)?;

        if current_mode.application_mode() == ApplicationMode::MainApplication {
            if enter_flash_mode {
                info!(
                    "Switching to flash mode... (current mode: {})",
                    current_mode.application_mode()
                );
                self.send_command(WriteSwitchApplicationCommand::new(
                    SwitchApplicationOption::BootApplication,
                ))?;

                // Give device time to switch modes
                std::thread::sleep(Duration::from_secs(2));

                let current_mode = self.send_command(ReadModeCommand)?;
                if current_mode.application_mode() != ApplicationMode::BootRom
                    && current_mode.application_mode() != ApplicationMode::SecondaryBootApplication
                {
                    return Err(Dlpc8445Error::general(format!(
                        "Failed to switch to flash mode after mode switch command (current mode: {})",
                        current_mode.application_mode(),
                    )));
                }

                info!(
                    "Successfully switched to flash mode ({})",
                    current_mode.application_mode()
                );
            } else {
                return Err(Dlpc8445Error::general(format!(
                    "Device is not in flash mode (current mode: {}). Use --enter-flash-mode to switch.",
                    current_mode.application_mode()
                )));
            }
        } else {
            info!(
                "Device is in flash mode (current mode: {})",
                current_mode.application_mode()
            );
        }

        Ok(())
    }

    pub fn erase_session(&mut self, flash_state: &mut FlashState) -> Result<()> {
        self.unlock_flash()?;

        while !flash_state.is_done() {
            let sector = flash_state.current_sector();
            info!(
                "Erasing sector {} at 0x{:08X}",
                sector.idx, sector.start_addr
            );
            self.erase_sector(sector)?;
            info!(
                "Done erasing sector {} at 0x{:08X}",
                sector.idx, sector.start_addr
            );
            flash_state.advance_sector();
        }
        info!("All sectors erased successfully");

        Ok(())
    }
}
