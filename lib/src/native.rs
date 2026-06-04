// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use std::time::Duration;

use log::{info, trace};
use nusb::{Device, ErrorKind, transfer::TransferError};
use nusb::{
    io::{EndpointRead, EndpointWrite},
    transfer::{Bulk, In, Out},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    Dlpc8445Error, Result,
    dlpc8445::{
        BULK_IN_ENDPOINT, BULK_MAX_PACKET_SIZE, BULK_OUT_ENDPOINT, ConnectionBackend, Dlpc8445Con,
        PRODUCT_ID, SendCommand, VENDOR_ID,
    },
    protocol::{Command, ResponsePacket, ResponsePayload},
    sleep,
};

pub struct NativeConnection {
    writer: EndpointWrite<Bulk>,
    reader: EndpointRead<Bulk>,
    checksum_present: bool,
    device: Device,
}

async fn query_device_impl(wait: bool) -> Result<NativeConnection> {
    let di = loop {
        let device = nusb::list_devices()
            .await?
            .find(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID);

        if let Some(device) = device {
            info!("DLPC 8445 device found");
            break device;
        }

        if !wait {
            return Err(Dlpc8445Error::general("No device found!".to_string()));
        }

        sleep(Duration::from_millis(100)).await;
    };

    let device = di.open().await?;
    let interface = device.claim_interface(0).await?;

    let writer = interface
        .endpoint::<Bulk, Out>(BULK_OUT_ENDPOINT)?
        .writer(BULK_MAX_PACKET_SIZE)
        .with_write_timeout(Duration::from_millis(500));

    let reader = interface
        .endpoint::<Bulk, In>(BULK_IN_ENDPOINT)?
        .reader(BULK_MAX_PACKET_SIZE)
        .with_read_timeout(Duration::from_secs(1));

    Ok(NativeConnection {
        writer,
        reader,
        checksum_present: false,
        device,
    })
}

pub async fn query_for_device() -> Option<NativeConnection> {
    query_device_impl(false).await.ok()
}

pub async fn wait_for_device() -> Result<NativeConnection> {
    query_device_impl(true).await
}

impl ConnectionBackend for NativeConnection {
    async fn request_device_access() -> Result<Dlpc8445Con<Self>> {
        Self::wait_for_device().await // Native doesn't need explicit access request
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

impl SendCommand for NativeConnection {
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

        self.writer.write_all(&encoded).await?;
        self.writer.flush_end_async().await?;

        let mut response = Vec::new();
        let mut reader = self.reader.until_short_packet();
        reader.read_to_end(&mut response).await?;
        reader
            .consume_end()
            .map_err(|err| Dlpc8445Error::general(err.to_string()))?;

        command
            .decode::<R>(&response)
            .and_then(|resp| R::decode(resp.data))
    }

    fn set_checksum_present(&mut self, checksum_present: bool) {
        self.checksum_present = checksum_present;
    }
}

impl From<nusb::Error> for Dlpc8445Error {
    fn from(err: nusb::Error) -> Self {
        if err.kind() == ErrorKind::Disconnected {
            Self::UsbDisconnected
        } else {
            Self::general(err)
        }
    }
}

impl From<nusb::transfer::TransferError> for Dlpc8445Error {
    fn from(err: nusb::transfer::TransferError) -> Self {
        if matches!(err, TransferError::Disconnected | TransferError::Cancelled) {
            Self::UsbDisconnected
        } else {
            Self::general(err)
        }
    }
}
