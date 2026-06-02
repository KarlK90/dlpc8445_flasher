// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use std::time::Duration;

use log::{info, trace};
use webusb_web::{OpenUsbDevice, Usb, UsbDeviceFilter};

use crate::sleep;

use crate::{
    Dlpc8445Error, Result,
    dlpc8445::{BULK_MAX_PACKET_SIZE, PRODUCT_ID, SendCommand, VENDOR_ID},
    protocol::{Command, ResponsePacket, ResponsePayload},
};

pub struct WebUsbConnection {
    device: OpenUsbDevice,
    checksum_present: bool,
}

pub async fn wait_for_device() -> Result<WebUsbConnection> {
    let usb = Usb::new()?;

    let device = loop {
        let mut devices = usb.devices().await;

        if let Some(device) = devices.pop() {
            info!("DLPC8445 device found");
            break device;
        }

        sleep(Duration::from_millis(100)).await;
    };

    let device = device.open().await?;
    device.select_configuration(1).await?;
    device.claim_interface(0).await?;

    Ok(WebUsbConnection {
        device,
        checksum_present: false,
    })
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

        let response = self
            .device
            .transfer_in(1, BULK_MAX_PACKET_SIZE as u32)
            .await?
            .to_vec();

        command
            .decode::<R>(&response)
            .and_then(|resp| R::decode(resp.data))
    }

    fn set_checksum_present(&mut self, checksum_present: bool) {
        self.checksum_present = checksum_present;
    }
}

impl From<webusb_web::Error> for Dlpc8445Error {
    fn from(err: webusb_web::Error) -> Self {
        match err.kind() {
            webusb_web::ErrorKind::Disconnected | webusb_web::ErrorKind::Transfer => {
                Dlpc8445Error::UsbDisconnected
            }
            _ => Dlpc8445Error::general(err.to_string()),
        }
    }
}
