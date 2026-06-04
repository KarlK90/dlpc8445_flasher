// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use std::time::Duration;

use log::{debug, trace, warn};
use tokio::select;
use webusb_web::{OpenUsbDevice, Usb, UsbDeviceFilter};

use crate::sleep;

use crate::{
    Dlpc8445Error, Result,
    dlpc8445::{
        BULK_MAX_PACKET_SIZE, ConnectionBackend, Dlpc8445Con, PRODUCT_ID, SendCommand, VENDOR_ID,
    },
    protocol::{Command, ResponsePacket, ResponsePayload},
};

pub struct WebUsbConnection {
    device: OpenUsbDevice,
    checksum_present: bool,
}

pub async fn wait_for_device() -> Result<WebUsbConnection> {
    let usb = Usb::new()?;

    let device = 'outer: loop {
        for device in usb.devices().await.into_iter() {
            if device.opened() {
                dbg!("Device already opened, skipping");
                continue;
            }
            match device.open().await {
                Ok(device) => {
                    debug!("Device found");
                    break 'outer device;
                }
                Err(err) => warn!("Failed to open device: {}", err),
            }
        }
        sleep(Duration::from_millis(100)).await;
    };

    device.select_configuration(1).await?;
    device.claim_interface(0).await?;

    debug!("Device opened!");

    Ok(WebUsbConnection {
        device,
        checksum_present: false,
    })
}

pub async fn query_for_device() -> Option<WebUsbConnection> {
    let usb = Usb::new().ok()?;

    let devices = usb.devices().await;

    for device in devices.into_iter() {
        if device.opened() {
            dbg!("Device already opened, skipping");
            continue;
        }
        match device.open().await {
            Ok(device) => {
                debug!("Device found");
                device.select_configuration(1).await.ok()?;
                device.claim_interface(0).await.ok()?;
                return Some(WebUsbConnection {
                    device,
                    checksum_present: false,
                });
            }
            Err(err) => warn!("Failed to open device: {}", err),
        }
    }
    None
}

pub async fn request_device_access() -> Result<WebUsbConnection> {
    let usb = Usb::new()?;
    let device = usb
        .request_device([UsbDeviceFilter::new()
            .with_vendor_id(VENDOR_ID)
            .with_product_id(PRODUCT_ID)])
        .await?;

    let device = device.open().await?;
    device.select_configuration(1).await?;
    device.claim_interface(0).await?;

    Ok(WebUsbConnection {
        device,
        checksum_present: false,
    })
}

impl WebUsbConnection {
    pub async fn reset(&mut self) -> Result<()> {
        self.device.reset().await?;
        Ok(())
    }
}

impl SendCommand for WebUsbConnection {
    async fn send_command<T, R>(&mut self, command: T) -> Result<R>
    where
        T: Command<ResponsePacket<R> = ResponsePacket<R>>,
        R: ResponsePayload,
    {
        trace!("Sending command: {:?}", command);
        let command = command
            .into_packet()?
            .set_checksum_present(self.checksum_present);
        trace!("Command packet: {:#?}", command);
        let encoded = command.encode()?;

        self.device.transfer_out(1, &encoded).await?;

        let response = select! {
            resp = self
                .device
                .transfer_in(1, BULK_MAX_PACKET_SIZE as u32) => {
                    Ok(resp?.to_vec())
                }
            _ = sleep(Duration::from_secs(1)) => {
                Err(Dlpc8445Error::UsbDisconnected)
            }
        };

        command
            .decode::<R>(&response?)
            .and_then(|resp| R::decode(resp.data))
    }

    fn set_checksum_present(&mut self, checksum_present: bool) {
        self.checksum_present = checksum_present;
    }
}

impl From<webusb_web::Error> for Dlpc8445Error {
    fn from(err: webusb_web::Error) -> Self {
        match err.kind() {
            webusb_web::ErrorKind::Disconnected
            | webusb_web::ErrorKind::Transfer
            | webusb_web::ErrorKind::Stall => Dlpc8445Error::UsbDisconnected,
            _ => Dlpc8445Error::general(err.to_string()),
        }
    }
}

impl ConnectionBackend for WebUsbConnection {
    async fn request_device_access() -> Result<Dlpc8445Con<Self>> {
        Ok(Dlpc8445Con::new(request_device_access().await?))
    }

    async fn wait_for_device() -> Result<Dlpc8445Con<Self>> {
        Ok(Dlpc8445Con::new(wait_for_device().await?))
    }

    async fn query_for_device() -> Option<Dlpc8445Con<Self>> {
        Some(Dlpc8445Con::new(query_for_device().await?))
    }

    async fn reset(&mut self) -> Result<()> {
        Ok(self.device.reset().await?)
    }
}
