// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use std::{
    cell::RefCell,
    fmt::{Display, Formatter},
    time::Duration,
};

use log::{error, info, warn};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    Dlpc8445Error, Result,
    dlpc8445::{ConnectionBackend, Dlpc8445Con, SendCommand},
    flash::{FLASH_SECTOR_SIZE, FlashState},
    protocol::ApplicationMode,
    sleep,
};

#[cfg(target_family = "wasm")]
use crate::webusb::WebUsbConnection;

const MAX_SECTOR_REPROGRAM_ATTEMPTS: usize = 3;

#[derive(Debug, Clone)]
pub enum RunnerCommand {
    RequestDeviceAccess,
    LoadImage {
        image: FlashState,
    },
    StartAction {
        action: RunnerAction,
        enter_flash_mode: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerAction {
    Flash,
    Erase,
    Verify,
}

impl Display for RunnerAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let action = match self {
            RunnerAction::Flash => "Flash",
            RunnerAction::Erase => "Erase",
            RunnerAction::Verify => "Verify",
        };
        write!(f, "{}", action)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    Disconnected,
    ConnectedApplication,
    ConnectedFlashMode,
    Connected,
}

impl From<ApplicationMode> for DeviceState {
    fn from(mode: ApplicationMode) -> Self {
        match mode {
            ApplicationMode::MainApplication => DeviceState::ConnectedApplication,
            ApplicationMode::BootRom | ApplicationMode::SecondaryBootApplication => {
                DeviceState::ConnectedFlashMode
            }
            ApplicationMode::Unknown => DeviceState::Connected,
        }
    }
}

impl Display for DeviceState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            DeviceState::Disconnected => "Disconnected",
            DeviceState::ConnectedApplication => "Connected (Application Mode)",
            DeviceState::ConnectedFlashMode => "Connected (Flash Mode)",
            DeviceState::Connected => "Connected",
        };
        write!(f, "{}", status)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerState {
    Idle,
    Running,
    WaitingForReconnect,
    Done,
    Error,
}

impl Display for RunnerState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let state = match self {
            RunnerState::Idle => "Idle",
            RunnerState::Running => "Running",
            RunnerState::WaitingForReconnect => "Waiting for reconnect",
            RunnerState::Done => "Done",
            RunnerState::Error => "Error",
        };
        write!(f, "{}", state)
    }
}

#[derive(Debug, Clone)]
pub enum RunnerEvent {
    DeviceStateUpdate(DeviceState),
    ProgressUpdate(ActionProgress),
    RunnerStateUpdate(RunnerState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionProgress {
    pub current: usize,
    pub total: usize,
}

pub struct Runner<T: ConnectionBackend> {
    dlpc: RefCell<Option<Dlpc8445Con<T>>>,
    flash_state: RefCell<Option<FlashState>>,
    event_tx: UnboundedSender<RunnerEvent>,
    command_rx: UnboundedReceiver<RunnerCommand>,
}

impl<T: ConnectionBackend> Runner<T> {
    pub fn send_event(&self, event: RunnerEvent) -> Result<()> {
        self.event_tx
            .send(event)
            .map_err(|e| Dlpc8445Error::general(e.to_string()))
    }

    pub fn new(
        event_tx: UnboundedSender<RunnerEvent>,
        command_rx: UnboundedReceiver<RunnerCommand>,
    ) -> Self {
        Self {
            dlpc: RefCell::new(None),
            flash_state: RefCell::new(None),
            event_tx,
            command_rx,
        }
    }

    pub async fn idle_task(&mut self) {
        let mut lost_connection = true;

        if let Some(dlpc) = self.dlpc.borrow_mut().as_mut() {
            if let Ok(mode) = dlpc.read_mode().await {
                let _ = self.send_event(RunnerEvent::DeviceStateUpdate(mode.into()));
                lost_connection = false;
            }
        }

        if lost_connection {
            self.dlpc.replace(None);
            let _ = self.send_event(RunnerEvent::DeviceStateUpdate(DeviceState::Disconnected));
            if let Some(dlpc) = T::query_for_device().await {
                self.dlpc.replace(Some(dlpc));
                let _ = self.send_event(RunnerEvent::DeviceStateUpdate(DeviceState::Connected));
            }
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            let command = tokio::select! {
                cmd = self.command_rx.recv() => {
                    let Some(cmd) = cmd else {
                        return Ok(()); // Channel closed
                    };
                    Some(cmd)
                }
                _ = sleep(Duration::from_millis(100)) => {
                    self.idle_task().await;
                    None
                }
            };

            let Some(command) = command else {
                continue;
            };

            match command {
                RunnerCommand::RequestDeviceAccess => match T::request_device_access().await {
                    Ok(dlpc) => {
                        self.dlpc.replace(Some(dlpc));
                        self.send_event(RunnerEvent::DeviceStateUpdate(DeviceState::Connected))?;
                    }
                    Err(err) => {
                        error!("Failed to request device access: {err}");
                    }
                },
                RunnerCommand::LoadImage { image } => {
                    self.send_event(RunnerEvent::ProgressUpdate(ActionProgress {
                        current: 0,
                        total: image.sectors().len(),
                    }))?;
                    *self.flash_state.borrow_mut() = Some(image);
                }
                RunnerCommand::StartAction {
                    action,
                    enter_flash_mode,
                } => {
                    let Some(ref mut flash_state) = *self.flash_state.borrow_mut() else {
                        error!("No image loaded; cannot start action {action}");
                        continue;
                    };
                    let Some(mut dlpc) = self.dlpc.borrow_mut().take() else {
                        error!("No active device connection; cannot start action {action}");
                        continue;
                    };

                    flash_state.reset();

                    self.send_event(RunnerEvent::RunnerStateUpdate(RunnerState::Running))?;

                    loop {
                        match self
                            .run_session(action, &mut dlpc, flash_state, enter_flash_mode)
                            .await
                        {
                            Err(Dlpc8445Error::UsbDisconnected) => {
                                warn!("DLPC 8445 disconnected");
                                flash_state.reset_current_sector();

                                self.send_event(RunnerEvent::DeviceStateUpdate(
                                    DeviceState::Disconnected,
                                ))?;
                                self.send_event(RunnerEvent::RunnerStateUpdate(
                                    RunnerState::WaitingForReconnect,
                                ))?;

                                info!("Waiting for device to reconnect...");
                                dlpc = match T::wait_for_device().await {
                                    Ok(dlpc) => dlpc,
                                    Err(err) => {
                                        error!("Error while waiting for device: {err}");
                                        self.send_event(RunnerEvent::RunnerStateUpdate(
                                            RunnerState::Error,
                                        ))?;
                                        break;
                                    }
                                };

                                self.send_event(RunnerEvent::DeviceStateUpdate(
                                    DeviceState::Connected,
                                ))?;
                                self.send_event(RunnerEvent::RunnerStateUpdate(
                                    RunnerState::Running,
                                ))?;
                            }
                            Err(err) => {
                                error!("{}", err);
                                self.send_event(RunnerEvent::RunnerStateUpdate(
                                    RunnerState::Error,
                                ))?;
                                break;
                            }
                            Ok(msg) => {
                                info!("{msg}");
                                self.send_event(RunnerEvent::RunnerStateUpdate(RunnerState::Done))?;
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    pub async fn run_session(
        &self,
        action: RunnerAction,
        dlpc: &mut Dlpc8445Con<T>,
        flash_state: &mut FlashState,
        enter_flash_mode: bool,
    ) -> std::result::Result<String, Dlpc8445Error> {
        dlpc.verify_flash_mode(enter_flash_mode).await?;

        let dlpc_info = dlpc.query_info().await?;

        self.send_event(RunnerEvent::DeviceStateUpdate(dlpc_info.mode.into()))?;

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

        match action {
            RunnerAction::Flash => self.flash_session(dlpc, flash_state).await,
            RunnerAction::Erase => self.erase_session(dlpc, flash_state).await,
            RunnerAction::Verify => self.validation_session(dlpc, flash_state).await,
        }
    }

    pub async fn flash_session(
        &self,
        dlpc: &mut Dlpc8445Con<T>,
        flash_state: &mut FlashState,
    ) -> Result<String> {
        dlpc.unlock_flash().await?;

        if flash_state.header_sector_needs_invalidation() {
            info!("Erasing first sector on flash to invalidate image");
            let header = flash_state.header_sector();
            dlpc.erase_sector(header).await?;
            info!(
                "Flashing sectors in reverse (from last to first) to ensure boot rom fallback for partial flashed images"
            );
            flash_state.reverse();
        }

        let total_sectors = flash_state.sectors().len();

        while !flash_state.is_done() {
            self.send_event(RunnerEvent::ProgressUpdate(ActionProgress {
                current: flash_state.current_sector_index(),
                total: total_sectors,
            }))?;

            let sector = flash_state.current_sector();

            if dlpc.validate_sector(sector).await.is_ok() {
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
                    dlpc.erase_sector(sector).await?;
                }

                if !sector.is_programmed() {
                    info!(
                        "Programming sector {} at 0x{:08X}",
                        sector.idx, sector.start_addr,
                    );
                    dlpc.program_sector(sector).await?;
                }

                info!(
                    "Validating sector {} at 0x{:08X}-0x{:08X}",
                    sector.idx, sector.start_addr, sector.end_addr
                );

                if let Err(err) = dlpc.validate_sector(sector).await {
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

        self.send_event(RunnerEvent::ProgressUpdate(ActionProgress {
            current: total_sectors,
            total: total_sectors,
        }))?;

        dlpc.lock_flash().await?;
        Ok("Flash programming complete!".to_string())
    }

    pub async fn validation_session(
        &self,
        dlpc: &mut Dlpc8445Con<T>,
        flash_state: &mut FlashState,
    ) -> Result<String> {
        let total_sectors = flash_state.sectors().len();

        while !flash_state.is_done() {
            self.send_event(RunnerEvent::ProgressUpdate(ActionProgress {
                current: flash_state.current_sector_index(),
                total: total_sectors,
            }))?;

            let sector = flash_state.current_sector();

            match dlpc.validate_sector(sector).await {
                Ok(_) => info!("Sector {}: valid", sector.idx),
                Err(err) => error!("Sector {}: invalid {err}", sector.idx),
            }
            flash_state.advance_sector();
        }

        self.send_event(RunnerEvent::ProgressUpdate(ActionProgress {
            current: total_sectors,
            total: total_sectors,
        }))?;

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

    pub async fn erase_session(
        &self,
        dlpc: &mut Dlpc8445Con<T>,
        flash_state: &mut FlashState,
    ) -> Result<String> {
        dlpc.unlock_flash().await?;

        let total_sectors = flash_state.sectors().len();

        while !flash_state.is_done() {
            self.send_event(RunnerEvent::ProgressUpdate(ActionProgress {
                current: flash_state.current_sector_index(),
                total: total_sectors,
            }))?;

            let sector = flash_state.current_sector();
            info!(
                "Erasing sector {} at 0x{:08X}",
                sector.idx, sector.start_addr
            );
            dlpc.erase_sector(sector).await?;
            info!("Done erasing sector {}", sector.idx);
            flash_state.advance_sector();
        }

        self.send_event(RunnerEvent::ProgressUpdate(ActionProgress {
            current: total_sectors,
            total: total_sectors,
        }))?;

        Ok("All sectors erased successfully".to_string())
    }
}
