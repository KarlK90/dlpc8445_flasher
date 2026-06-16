// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use std::time::Duration;

use log::info;

use crate::{
    Dlpc8445Error, Result,
    flash::{FLASH_PAGE_SIZE, FlashSector},
    protocol::{
        ApplicationMode, BootHoldReasonResponse, FlashIdResponse, FlashSectorInformationResponse,
        ReadBootHoldReasonCommand, ReadExtendedSoftwareVersionCommand, ReadFlashIdCommand,
        ReadGetFlashSectorInformationCommand, ReadModeCommand, ReadVersionCommand,
        SwitchApplicationOption, WriteSwitchApplicationCommand,
    },
    runner::DeviceState,
    sleep,
};
use crate::{
    flash::{FLASH_PAGE_PROGRAM_TIME, FLASH_SECTOR_ERASE_TIME},
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

pub trait ConnectionBackend: SendCommand + Sized {
    fn request_device_access() -> impl Future<Output = Result<Dlpc8445Con<Self>>>;
    fn wait_for_device() -> impl Future<Output = Result<Dlpc8445Con<Self>>>;
    fn query_for_device() -> impl Future<Output = Option<Dlpc8445Con<Self>>>;
    fn reset(&mut self) -> impl Future<Output = Result<()>>;
}

#[derive(Debug)]
pub struct Dlpc8445Info {
    pub boot_hold_reason: BootHoldReasonResponse,
    pub flash_id: FlashIdResponse,
    pub flash_sector: FlashSectorInformationResponse,
    pub mode: ApplicationMode,
}

impl<T: ConnectionBackend> Dlpc8445Con<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, info: None }
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

    pub async fn read_mode(&mut self) -> Result<ApplicationMode> {
        Ok(self
            .inner
            .send_command(ReadModeCommand)
            .await?
            .application_mode())
    }

    pub async fn read_device_state(&mut self) -> Result<DeviceState> {
        let mode = self
            .inner
            .send_command(ReadModeCommand)
            .await?
            .application_mode();

        let device_state = match mode {
            ApplicationMode::MainApplication => DeviceState::ConnectedApplication {
                version: self.inner.send_command(ReadVersionCommand).await?,
                extended_version: self
                    .inner
                    .send_command(ReadExtendedSoftwareVersionCommand)
                    .await?,
            },
            ApplicationMode::BootRom | ApplicationMode::SecondaryBootApplication => {
                DeviceState::ConnectedFlashMode
            }
            ApplicationMode::Unknown => DeviceState::Connected,
        };

        Ok(device_state)
    }

    pub async fn unlock_flash(&mut self) -> Result<()> {
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

    pub async fn lock_flash(&mut self) -> Result<()> {
        self.inner
            .send_command(WriteUnlockFlashForUpdateCommand::lock())
            .await
    }

    pub async fn initialize_flash_rw(
        &mut self,
        start_address: usize,
        num_bytes: usize,
    ) -> Result<()> {
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

    pub async fn program_sector(&mut self, sector: &mut FlashSector) -> Result<()> {
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

    pub async fn erase_sector(&mut self, sector: &mut FlashSector) -> Result<()> {
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

    pub async fn validate_sector(&mut self, sector: &mut FlashSector) -> Result<()> {
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

    pub async fn read_sector_checksum(&mut self, sector: &FlashSector) -> Result<ChecksumResponse> {
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
                // DLPC will disconnect at this point
                self.switch_mode(SwitchApplicationOption::BootApplication)
                    .await?;

                // Give some time to settle and force the upper layer to
                // re-establish the USB connection
                sleep(Duration::from_secs(2)).await;
                return Err(Dlpc8445Error::UsbDisconnected);
            } else {
                return Err(Dlpc8445Error::RunnerAbort(format!(
                    "Device is not in flash mode (current mode: {}). Use --enter-flash-mode to switch.",
                    current_mode
                )));
            }
        } else {
            info!("Device is in flash mode (current mode: {})", current_mode);
        }

        Ok(())
    }

    pub async fn switch_mode(&mut self, mode: SwitchApplicationOption) -> Result<()> {
        self.inner
            .send_command(WriteSwitchApplicationCommand::new(mode))
            .await
    }

    pub async fn reset(&mut self) -> Result<()> {
        self.inner.reset().await
    }
}
