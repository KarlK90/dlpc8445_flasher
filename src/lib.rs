pub mod dlpc8445;
pub mod flash;
pub mod protocol;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Dlpc8445Error>;

#[derive(Debug, Error)]
pub enum Dlpc8445Error {
    #[error("USB disconnected")]
    UsbDisconnected,
    #[error(transparent)]
    Protocol(#[from] binrw::Error),
    #[error("{0}")]
    General(String),
}

impl Dlpc8445Error {
    pub fn general(message: impl Into<String>) -> Self {
        Self::General(message.into())
    }
}

impl From<std::io::Error> for Dlpc8445Error {
    fn from(value: std::io::Error) -> Self {
        if matches!(
            value.kind(),
            std::io::ErrorKind::NotConnected
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::BrokenPipe
        ) {
            Self::UsbDisconnected
        } else {
            Self::general(value.to_string())
        }
    }
}

impl From<nusb::Error> for Dlpc8445Error {
    fn from(value: nusb::Error) -> Self {
        if value.kind() == nusb::ErrorKind::Disconnected {
            Self::UsbDisconnected
        } else {
            Self::general(value.to_string())
        }
    }
}

impl From<nusb::transfer::TransferError> for Dlpc8445Error {
    fn from(value: nusb::transfer::TransferError) -> Self {
        if matches!(value, nusb::transfer::TransferError::Disconnected) {
            Self::UsbDisconnected
        } else {
            Self::general(value.to_string())
        }
    }
}

pub fn fletcher_64(data: impl AsRef<[u8]>) -> u64 {
    let mut simple_checksum = 0u32;
    let mut sum_of_simple_checksum = 0u32;

    for &byte in data.as_ref() {
        simple_checksum = simple_checksum.wrapping_add(byte as u32);
        sum_of_simple_checksum = sum_of_simple_checksum.wrapping_add(simple_checksum);
    }

    (simple_checksum as u64) << 32 | sum_of_simple_checksum as u64
}

pub struct Checksum<T> {
    inner: T,
    bytes: Vec<u8>,
}

impl<T> Checksum<T> {
    pub fn new(inner: T) -> Self {
        Self {
            bytes: Vec::new(),
            inner,
        }
    }

    pub fn get_u8(&self) -> u8 {
        fletcher_64(&self.bytes) as u8
    }

    pub fn get_u8_until_last_byte(&self) -> u8 {
        fletcher_64(&self.bytes[..self.bytes.len() - 1]) as u8
    }
}

impl<T: binrw::io::Write> binrw::io::Write for Checksum<T> {
    fn write(&mut self, buf: &[u8]) -> binrw::io::Result<usize> {
        self.bytes.extend_from_slice(buf);
        self.inner.write(buf)
    }

    fn flush(&mut self) -> binrw::io::Result<()> {
        self.inner.flush()
    }
}

impl<T: binrw::io::Seek> binrw::io::Seek for Checksum<T> {
    fn seek(&mut self, pos: binrw::io::SeekFrom) -> binrw::io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl<T: binrw::io::Read> binrw::io::Read for Checksum<T> {
    fn read(&mut self, buf: &mut [u8]) -> binrw::io::Result<usize> {
        let len = self.inner.read(buf)?;
        self.bytes.extend_from_slice(buf);
        Ok(len)
    }
}
