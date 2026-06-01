// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use std::time::Duration;

use log::{error, info, warn};

use crate::{
    Dlpc8445Error, Result,
    flash::{FLASH_PAGE_SIZE, FlashSector},
    protocol::{
        ApplicationMode, BootHoldReasonResponse, FlashIdResponse, FlashSectorInformationResponse,
        ReadBootHoldReasonCommand, ReadFlashIdCommand, ReadGetFlashSectorInformationCommand,
        ReadModeCommand, SwitchApplicationOption, WriteSwitchApplicationCommand,
    },
    sleep,
};
use crate::{
    flash::{FLASH_PAGE_PROGRAM_TIME, FLASH_SECTOR_ERASE_TIME, FlashState},
    protocol::{
        ChecksumResponse, Command, FlashWriteCommand, ReadChecksumCommand,
        ReadUnlockFlashForUpdateCommand, ResponsePacket, ResponsePayload, WriteEraseSectorCommand,
        WriteInitializeFlashReadWriteSettingsCommand, WriteUnlockFlashForUpdateCommand,
    },
};

pub const VENDOR_ID: u16 = 0x0451;
pub const PRODUCT_ID: u16 = 0x8430;
pub const BULK_OUT_ENDPOINT: u8 = 0x1;
pub const BULK_IN_ENDPOINT: u8 = 0x81;
pub const BULK_MAX_PACKET_SIZE: usize = 512;
const MAX_SECTOR_REPROGRAM_ATTEMPTS: usize = 3;

pub struct Dlpc8445Con<T: SendCommand> {
    inner: T,
    info: Option<Dlpc8445Info>,
}

pub trait SendCommand {
    fn send_command<T, R>(&mut self, command: T) -> impl Future<Output = Result<R>>
    where
        T: Command<ResponsePacket<R> = ResponsePacket<R>> + Send,
        R: ResponsePayload;

    fn set_checksum_present(&mut self, checksum_present: bool);
}

pub struct Dlpc8445Info {
    pub boot_hold_reason: BootHoldReasonResponse,
    pub flash_id: FlashIdResponse,
    pub flash_sector: FlashSectorInformationResponse,
    pub mode: ApplicationMode,
}

impl<T: SendCommand> Dlpc8445Con<T> {
    #[cfg(not(target_family = "wasm"))]
    pub async fn wait_for_device() -> Result<Dlpc8445Con<crate::native::NativeConnection>> {
        Ok(Dlpc8445Con {
            inner: crate::native::wait_for_device().await?,
            info: None,
        })
    }

    #[cfg(target_family = "wasm")]
    pub async fn wait_for_device() -> Result<Dlpc8445Con<crate::webusb::WebUsbConnection>> {
        Ok(Dlpc8445Con {
            inner: crate::webusb::wait_for_device().await?,
            info: None,
        })
    }

    pub async fn query_info(&mut self) -> Result<&Dlpc8445Info> {
        let boot_hold_reason = self.inner.send_command(ReadBootHoldReasonCommand).await?;
        let flash_info = self.inner.send_command(ReadFlashIdCommand).await?;
        let flash_sector_info = self
            .inner
            .send_command(ReadGetFlashSectorInformationCommand)
            .await?;
        let mode = self
            .inner
            .send_command(ReadModeCommand)
            .await?
            .application_mode();

        // Bug in boot rom, no response if checksum is present!
        let checksum_present = matches!(
            mode,
            ApplicationMode::MainApplication | ApplicationMode::SecondaryBootApplication
        );
        self.inner.set_checksum_present(checksum_present);

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

    pub async fn flash_session(&mut self, flash_state: &mut FlashState) -> Result<String> {
        self.unlock_flash().await?;

        if flash_state.header_sector_needs_invalidation() {
            info!("Erasing first sector on flash to invalidate image");
            let header = flash_state.header_sector();
            self.erase_sector(header).await?;
            info!(
                "Flashing sectors in reverse (from last to first) to ensure boot rom fallback for partial flashed images"
            );
            flash_state.reverse();
        }

        while !flash_state.is_done() {
            let sector = flash_state.current_sector();

            if self.validate_sector(sector).await.is_ok() {
                info!(
                    "Sector {} at 0x{:08X} already matches image",
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
                    self.erase_sector(sector).await?;
                }

                if !sector.is_programmed() {
                    info!(
                        "Programming sector {} at 0x{:08X}",
                        sector.idx, sector.start_addr,
                    );
                    self.program_sector(sector).await?;
                }

                info!(
                    "Validating sector {} at 0x{:08X}-0x{:08X}",
                    sector.idx, sector.start_addr, sector.end_addr
                );

                if let Err(err) = self.validate_sector(sector).await {
                    if reprogram_attempts >= MAX_SECTOR_REPROGRAM_ATTEMPTS {
                        return Err(Dlpc8445Error::general(format!(
                            "Validation failed for sector {} after {} reprogram attempts: {}",
                            sector.idx, MAX_SECTOR_REPROGRAM_ATTEMPTS, err
                        )));
                    }

                    reprogram_attempts += 1;
                    warn!(
                        "Validation failed for sector {}; erasing and reprogramming (attempt {}/{})",
                        sector.idx, reprogram_attempts, MAX_SECTOR_REPROGRAM_ATTEMPTS
                    );
                    sector.reset();
                }
            }

            info!("Sector {} complete", sector.idx);
            flash_state.advance_sector();
        }

        self.lock_flash().await?;
        Ok("Flash programming complete!".to_string())
    }

    pub async fn validation_session(&mut self, flash_state: &mut FlashState) -> Result<String> {
        while !flash_state.is_done() {
            let sector = flash_state.current_sector();

            match self.validate_sector(sector).await {
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
            Ok(msg)
        } else {
            Err(Dlpc8445Error::general(msg))
        }
    }

    async fn unlock_flash(&mut self) -> Result<()> {
        self.inner
            .send_command(WriteUnlockFlashForUpdateCommand::unlock())
            .await?;
        let unlock_state = self
            .inner
            .send_command(ReadUnlockFlashForUpdateCommand)
            .await?;

        if !unlock_state.is_unlocked() {
            return Err(Dlpc8445Error::general(
                "failed to unlock flash update commands",
            ));
        }
        Ok(())
    }

    async fn lock_flash(&mut self) -> Result<()> {
        self.inner
            .send_command(WriteUnlockFlashForUpdateCommand::lock())
            .await
    }

    async fn initialize_flash_rw(&mut self, start_address: usize, num_bytes: usize) -> Result<()> {
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

        self.inner.send_command(command).await?;
        Ok(())
    }

    async fn program_sector(&mut self, sector: &mut FlashSector) -> Result<()> {
        if sector.remaining() == 0 {
            return Ok(());
        }

        self.initialize_flash_rw(sector.start_addr + sector.current_addr, sector.remaining())
            .await?;

        while sector.current_addr < sector.len() {
            let next_pos = (sector.current_addr + FLASH_PAGE_SIZE).min(sector.len());
            let chunk = &sector.data[sector.current_addr..next_pos];

            self.inner
                .send_command(FlashWriteCommand {
                    data: chunk.to_vec(),
                })
                .await?;
            sleep(FLASH_PAGE_PROGRAM_TIME).await;
            sector.current_addr = next_pos;
        }

        Ok(())
    }

    async fn erase_sector(&mut self, sector: &mut FlashSector) -> Result<()> {
        let sector_address = sector.start_addr.try_into().map_err(|_| {
            Dlpc8445Error::general(format!(
                "flash sector start address 0x{:08X} does not fit into u32",
                sector.start_addr
            ))
        })?;
        self.inner
            .send_command(WriteEraseSectorCommand::new(sector_address))
            .await?;
        sleep(FLASH_SECTOR_ERASE_TIME).await;
        sector.mark_erased();
        Ok(())
    }

    async fn validate_sector(&mut self, sector: &mut FlashSector) -> Result<()> {
        let image_checksum = sector.checksum;
        let flash_checksum = self.read_sector_checksum(sector).await?.as_u64();

        if flash_checksum != image_checksum {
            return Err(Dlpc8445Error::general(format!(
                "verification failed for sector {} at 0x{:08X}: checksum mismatch (image=0x{:08X}, flash=0x{:08X})",
                sector.idx, sector.start_addr, image_checksum, flash_checksum
            )));
        }

        sector.verified = true;

        Ok(())
    }

    async fn read_sector_checksum(&mut self, sector: &FlashSector) -> Result<ChecksumResponse> {
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

        self.inner
            .send_command(ReadChecksumCommand {
                start_address,
                num_bytes,
            })
            .await
    }

    pub async fn verify_flash_mode(&mut self, enter_flash_mode: bool) -> Result<()> {
        let current_mode = self
            .inner
            .send_command(ReadModeCommand)
            .await?
            .application_mode();

        if !matches!(
            current_mode,
            ApplicationMode::BootRom | ApplicationMode::SecondaryBootApplication
        ) {
            if enter_flash_mode {
                info!(
                    "Switching to flash mode... (current mode: {})",
                    current_mode
                );
                self.inner
                    .send_command(WriteSwitchApplicationCommand::new(
                        SwitchApplicationOption::BootApplication,
                    ))
                    .await?;

                // Give device time to switch modes
                sleep(Duration::from_secs(2)).await;

                let current_mode = self
                    .inner
                    .send_command(ReadModeCommand)
                    .await?
                    .application_mode();

                if !matches!(
                    current_mode,
                    ApplicationMode::BootRom | ApplicationMode::SecondaryBootApplication
                ) {
                    return Err(Dlpc8445Error::general(format!(
                        "Failed to switch to flash mode after mode switch command (current mode: {})",
                        current_mode
                    )));
                }

                info!("Successfully switched to flash mode ({})", current_mode);
            } else {
                return Err(Dlpc8445Error::general(format!(
                    "Device is not in flash mode (current mode: {}). Use --enter-flash-mode to switch.",
                    current_mode
                )));
            }
        } else {
            info!("Device is in flash mode (current mode: {})", current_mode);
        }

        Ok(())
    }

    pub async fn erase_session(&mut self, flash_state: &mut FlashState) -> Result<String> {
        self.unlock_flash().await?;

        while !flash_state.is_done() {
            let sector = flash_state.current_sector();
            info!(
                "Erasing sector {} at 0x{:08X}",
                sector.idx, sector.start_addr
            );
            self.erase_sector(sector).await?;
            info!("Done erasing sector {}", sector.idx);
            flash_state.advance_sector();
        }
        Ok("All sectors erased successfully".to_string())
    }
}
