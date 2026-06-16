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
    dlpc8445::{ConnectionBackend, Dlpc8445Con},
    flash::{FLASH_SECTOR_SIZE, FlashState},
    protocol::{ExtendedSoftwareVersionResponse, SwitchApplicationOption, VersionResponse},
    sleep,
};

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
    SwitchMode {
        mode: SwitchApplicationOption,
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
    ConnectedApplication {
        version: VersionResponse,
        extended_version: ExtendedSoftwareVersionResponse,
    },
    ConnectedFlashMode,
    Connected,
}

impl Display for DeviceState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            DeviceState::Disconnected => "Disconnected",
            DeviceState::ConnectedApplication {
                version: _,
                extended_version: _,
            } => "Connected (Application Mode)",
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
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
    pub fn send_event(&self, event: RunnerEvent) {
        if self.event_tx.is_closed() {
            warn!("event channel is closed; cannot send event");
        }
        _ = self.event_tx.send(event);
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
            if let Ok(state) = dlpc.read_device_state().await {
                self.send_event(RunnerEvent::DeviceStateUpdate(state));
                lost_connection = false;
            }
        }

        if lost_connection {
            self.dlpc.replace(None);
            self.send_event(RunnerEvent::DeviceStateUpdate(DeviceState::Disconnected));
            if let Some(dlpc) = T::query_for_device().await {
                self.dlpc.replace(Some(dlpc));
                self.send_event(RunnerEvent::DeviceStateUpdate(DeviceState::Connected));
            }
        }
    }

    pub async fn run(&mut self) {
        loop {
            let command = tokio::select! {
                cmd = self.command_rx.recv() => {
                    let Some(cmd) = cmd else {
                        return; // Channel closed
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
                        self.send_event(RunnerEvent::DeviceStateUpdate(DeviceState::Connected));
                    }
                    Err(err) => {
                        error!("Failed to request device access: {err}");
                    }
                },
                RunnerCommand::SwitchMode { mode } => {
                    let Some(mut dlpc) = self.dlpc.borrow_mut().take() else {
                        error!("No active device connection; cannot switch mode {mode}");
                        continue;
                    };

                    if let Err(err) = dlpc.switch_mode(mode).await {
                        error!("Failed to switch mode {mode}: {err}");
                    }
                }
                RunnerCommand::LoadImage { image } => {
                    self.send_event(RunnerEvent::ProgressUpdate(ActionProgress {
                        current: 0,
                        total: image.sectors().len(),
                    }));
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

                    loop {
                        match self
                            .run_session(action, &mut dlpc, flash_state, enter_flash_mode)
                            .await
                        {
                            Err(err) => {
                                match err {
                                    Dlpc8445Error::UsbDisconnected => {
                                        warn!("DLPC 8445: disconnected");
                                        self.send_event(RunnerEvent::DeviceStateUpdate(
                                            DeviceState::Disconnected,
                                        ));
                                    }
                                    Dlpc8445Error::RunnerAbort(err) => {
                                        error!("{err}");
                                        self.send_event(RunnerEvent::RunnerStateUpdate(
                                            RunnerState::Error,
                                        ));
                                        self.dlpc.replace(Some(dlpc));
                                        break;
                                    }
                                    _ => {
                                        error!("DLPC 8445: {}", err);
                                        warn!("Resetting USB connecting");
                                        self.send_event(RunnerEvent::RunnerStateUpdate(
                                            RunnerState::Error,
                                        ));
                                        match dlpc.reset().await {
                                            Ok(_) => info!("USB connection reset successfully"),
                                            Err(err) => {
                                                error!("Failed to reset USB connection: {err}")
                                            }
                                        }
                                    }
                                }

                                self.send_event(RunnerEvent::DeviceStateUpdate(
                                    DeviceState::Disconnected,
                                ));
                                self.send_event(RunnerEvent::RunnerStateUpdate(
                                    RunnerState::WaitingForReconnect,
                                ));
                                info!("Waiting for device to reconnect...");

                                flash_state.reset_current_sector();
                                // Explicitly drop the connection before waiting to ensure OS frees the interface
                                drop(dlpc);

                                dlpc = match T::wait_for_device().await {
                                    Ok(dlpc) => dlpc,
                                    Err(err) => {
                                        error!("Error while waiting for device: {err}");
                                        self.send_event(RunnerEvent::RunnerStateUpdate(
                                            RunnerState::Error,
                                        ));
                                        break;
                                    }
                                };
                                self.send_event(RunnerEvent::DeviceStateUpdate(
                                    DeviceState::Connected,
                                ));
                                self.send_event(RunnerEvent::RunnerStateUpdate(
                                    RunnerState::Running,
                                ));
                            }
                            Ok(msg) => {
                                info!("{msg}");
                                self.send_event(RunnerEvent::RunnerStateUpdate(RunnerState::Done));
                                self.dlpc.replace(Some(dlpc));
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
        self.send_event(RunnerEvent::RunnerStateUpdate(RunnerState::Running));
        dlpc.verify_flash_mode(enter_flash_mode).await?;

        self.send_event(RunnerEvent::DeviceStateUpdate(
            dlpc.read_device_state().await?,
        ));

        let dlpc_info = dlpc.query_info().await?;

        if dlpc_info.flash_sector.sector_size as usize != FLASH_SECTOR_SIZE {
            return Err(Dlpc8445Error::RunnerAbort(format!(
                "controller reported an invalid flash sector size of {} bytes, expected {}",
                dlpc_info.flash_sector.sector_size, FLASH_SECTOR_SIZE
            )));
        }

        info!(
            "boot_hold_reason={} flash_id={{manufacturer: 0x{:02X}, device: 0x{:02X}, capacity: 0x{:04X}}} sector_size={}",
            dlpc_info.boot_hold_reason.reason,
            dlpc_info.flash_id.manufacturer,
            dlpc_info.flash_id.device,
            dlpc_info.flash_id.capacity,
            dlpc_info.flash_sector.sector_size,
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
        while let Some(sector) = flash_state.current_sector() {
            self.send_event(RunnerEvent::ProgressUpdate(ActionProgress {
                current: 0 + (total_sectors - sector.idx), // Flash progress is reversed
                total: total_sectors,
            }));
            if sector.checksum_unreliable {
                info!(
                    "Sector {} at 0x{:08X} checksum unreliable; force erasing and programming",
                    sector.idx, sector.start_addr
                );
            } else if dlpc.validate_sector(sector).await.is_ok() {
                info!(
                    "Sector {} at 0x{:08X} already matches image",
                    sector.idx, sector.start_addr
                );

                flash_state.advance_sector();
                continue;
            } else {
                info!(
                    "Sector {} checksum mismatch; erasing and programming",
                    sector.idx,
                );
            }

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
                        return Err(Dlpc8445Error::RunnerAbort(format!(
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

        dlpc.lock_flash().await?;
        Ok("Flash programming complete!".to_string())
    }

    pub async fn validation_session(
        &self,
        dlpc: &mut Dlpc8445Con<T>,
        flash_state: &mut FlashState,
    ) -> Result<String> {
        let total_sectors = flash_state.sectors().len();
        while let Some(sector) = flash_state.current_sector() {
            self.send_event(RunnerEvent::ProgressUpdate(ActionProgress {
                current: sector.idx,
                total: total_sectors,
            }));
            match dlpc.validate_sector(sector).await {
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
            Err(Dlpc8445Error::RunnerAbort(msg))
        }
    }

    pub async fn erase_session(
        &self,
        dlpc: &mut Dlpc8445Con<T>,
        flash_state: &mut FlashState,
    ) -> Result<String> {
        dlpc.unlock_flash().await?;

        let total_sectors = flash_state.sectors().len();
        while let Some(sector) = flash_state.current_sector() {
            self.send_event(RunnerEvent::ProgressUpdate(ActionProgress {
                current: sector.idx,
                total: total_sectors,
            }));
            info!(
                "Erasing sector {} at 0x{:08X}",
                sector.idx, sector.start_addr
            );
            dlpc.erase_sector(sector).await?;
            info!("Done erasing sector {}", sector.idx);
            flash_state.advance_sector();
        }

        Ok("All sectors erased successfully".to_string())
    }
}
