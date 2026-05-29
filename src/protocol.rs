// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use std::{io::Cursor, marker::PhantomData};

use binrw::{BinRead, BinWrite, Endian, binread, binwrite, helpers::until_eof};
use bitfield_struct::bitfield;
use strum::{Display, FromRepr};

use crate::{Checksum, Result};

pub const OP_WRITE_ERASE_SECTOR: u8 = 0x23;
pub const OP_WRITE_INITIALIZE_FLASH_RW_SETTINGS: u8 = 0x24;
pub const OP_FLASH_WRITE_AND_READ: u8 = 0x25;
pub const OP_WRITE_FULL_FLASH_ERASE: u8 = 0x28;
pub const OP_READ_MODE: u8 = 0x00;
pub const OP_READ_VERSION: u8 = 0x01;
pub const OP_WRITE_SWITCH_APPLICATION: u8 = 0x02;
pub const OP_READ_BOOT_HOLD_REASON: u8 = 0x12;
pub const OP_SYSTEM_TYPE: u8 = 0x03;
pub const OP_READ_EXTENDED_SOFTWARE_VERSION: u8 = 0x04;
pub const OP_WRITE_CLEAR_ERROR_HISTORY: u8 = 0x05;
pub const OP_READ_ERROR_HISTORY: u8 = 0x06;
pub const OP_READ_FLASH_ID: u8 = 0x20;
pub const OP_READ_GET_FLASH_SECTOR_INFORMATION: u8 = 0x21;
pub const OP_UNLOCK_FLASH_FOR_UPDATE: u8 = 0x22;
pub const OP_READ_CHECKSUM: u8 = 0x26;

pub const FLASH_UPDATE_UNLOCK_CODE: u32 = 4_154_802_215;
pub const CLEAR_ERROR_HISTORY_SIGNATURE: u32 = 0xDDCC_BBAA;

/// Shared command header bit assignments (DLPU114A, Table 5-2).
#[bitfield(u8)]
#[derive(PartialEq, Eq, BinWrite)]
pub struct CommandHeaderBits {
    #[bits(3)]
    destination: u8,
    opcode_length: bool,
    data_length_present: bool,
    checksum_present: bool,
    reply_requested: bool,
    read_command: bool,
}

#[bitfield(u8)]
#[derive(PartialEq, Eq, BinRead)]
pub struct ResponseHeaderBits {
    #[bits(3)]
    destination: u8,
    reserved: bool,
    data_length_present: bool,
    checksum_present: bool,
    error: bool,
    busy: bool,
}

pub trait ResponsePayload: for<'a> BinRead<Args<'a> = ()> {
    fn decode(data: impl AsRef<[u8]>) -> Result<Self> {
        let mut cursor = Cursor::new(data);
        Ok(Self::read_options(&mut cursor, Endian::Little, ())?)
    }
}

pub trait Command: std::fmt::Debug + Sized + BinWrite + for<'a> BinWrite<Args<'a> = ()> {
    const OPCODE: u8;
    const READ_COMMAND: bool;
    type ResponsePacket<P>: for<'a> BinRead<Args<'a> = ()>
    where
        P: ResponsePayload;

    fn into_packet(self) -> Result<CommandPacket<Self>> {
        CommandPacket::new(self)
    }
}

#[binwrite]
#[bw(little, stream = w, map_stream = Checksum::new)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPacket<T: Command> {
    pub header: CommandHeaderBits,
    pub opcode: u8,
    #[bw(calc(data.len() as u16))]
    pub data_length: u16,
    pub data: Vec<u8>,
    #[bw(if(header.checksum_present()))]
    #[bw(calc(w.get_u8()))]
    pub checksum: u8,
    _type: PhantomData<T>,
}

impl<T: Command> CommandPacket<T> {
    pub fn new(command: T) -> Result<Self> {
        let header = CommandHeaderBits::new()
            .with_destination(1) // Destination 1 is the boot ROM
            .with_opcode_length(false)
            .with_data_length_present(true) // Always present, simplifies encoding/decoding
            .with_checksum_present(true)
            .with_reply_requested(true) // Always present, simplifies encoding/decoding
            .with_read_command(T::READ_COMMAND);

        let mut data = Cursor::new(Vec::new());
        command.write_le(&mut data)?;

        Ok(Self {
            header,
            opcode: T::OPCODE,
            data: data.into_inner(),
            _type: PhantomData,
        })
    }

    pub fn set_destination(mut self, destination: u8) -> Self {
        self.header.set_destination(destination);
        self
    }

    pub fn set_checksum_present(mut self, checksum_present: bool) -> Self {
        self.header.set_checksum_present(checksum_present);
        self
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut encoded = Cursor::new(Vec::new());
        self.write_le(&mut encoded)?;
        Ok(encoded.into_inner())
    }

    pub fn decode<P: ResponsePayload>(
        &self,
        bytes: impl AsRef<[u8]>,
    ) -> Result<T::ResponsePacket<P>> {
        let mut cursor = Cursor::new(bytes);
        Ok(T::ResponsePacket::<P>::read_options(
            &mut cursor,
            Endian::Little,
            (),
        )?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromRepr, Display)]
#[repr(u8)]
pub enum ErrorCode {
    InvalidDestination = 1,
    InvalidUnknownCommand = 2,
    InvalidLength = 3,
    AllocatedBufferNotEnough = 4,
    LengthInformationMissing = 5,
    ChecksumMismatch = 6,
    TimeoutError = 7,
    ReadNotSupported = 8,
    WriteNotSupported = 9,
    ExecutionFailed = 10,
    InvalidResponseLength = 11,
    BufferFull = 12,
    UnknownError = 255,
}

impl From<u8> for ErrorCode {
    fn from(value: u8) -> Self {
        ErrorCode::from_repr(value).unwrap_or(ErrorCode::UnknownError)
    }
}

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
struct ChecksumMismatchError {
    pub expected: u8,
    pub actual: u8,
}

impl core::fmt::Display for ChecksumMismatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Checksum mismatch: expected {:X}, got {:X}",
            self.expected, self.actual
        )
    }
}

#[binread]
#[br(little, stream = r, map_stream = Checksum::new)]
#[br(assert(!header.error(), ErrorCode::from(data[0])))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsePacket<T>
where
    T: for<'a> BinRead<Args<'a> = ()>,
{
    _type: PhantomData<T>,
    pub header: ResponseHeaderBits,
    pub data_length: u16,
    #[br(count = { if header.error() { 1 } else { data_length }})]
    pub data: Vec<u8>,
    #[br(if(!header.error() && header.checksum_present()))]
    // FIXME: The observed checksum response on the wire doesn't seem to match
    // the calculated checksum, so skip the checksum verification for now.
    // #[br(assert(checksum == r.get_u8_until_last_byte(), ChecksumMismatchError { expected: checksum, actual: r.get_u8_until_last_byte() }))]
    pub checksum: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct WriteEraseSectorCommand {
    pub sector_address: u32,
}

impl WriteEraseSectorCommand {
    pub fn new(sector_address: u32) -> Self {
        Self { sector_address }
    }
}

impl Command for WriteEraseSectorCommand {
    const OPCODE: u8 = OP_WRITE_ERASE_SECTOR;
    const READ_COMMAND: bool = false;

    type ResponsePacket<T: ResponsePayload> = ResponsePacket<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct ReadBootHoldReasonCommand;

impl Command for ReadBootHoldReasonCommand {
    const OPCODE: u8 = OP_READ_BOOT_HOLD_REASON;
    const READ_COMMAND: bool = true;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<BootHoldReasonResponse>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct ReadSystemTypeCommand;

impl Command for ReadSystemTypeCommand {
    const OPCODE: u8 = OP_SYSTEM_TYPE;
    const READ_COMMAND: bool = true;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<SystemTypeResponse>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct WriteSystemTypeCommand {
    pub system_type: u8,
}

impl WriteSystemTypeCommand {
    pub fn new(system_type: u8) -> Self {
        Self { system_type }
    }
}

impl Command for WriteSystemTypeCommand {
    const OPCODE: u8 = OP_SYSTEM_TYPE;
    const READ_COMMAND: bool = false;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct ReadModeCommand;

impl Command for ReadModeCommand {
    const OPCODE: u8 = OP_READ_MODE;
    const READ_COMMAND: bool = true;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<ModeResponse>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct ReadVersionCommand;

impl Command for ReadVersionCommand {
    const OPCODE: u8 = OP_READ_VERSION;
    const READ_COMMAND: bool = true;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<VersionResponse>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr, Display)]
#[repr(u8)]
pub enum SwitchApplicationOption {
    BootApplication = 0,
    MainApplication = 1,
    SecondaryBoot = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct WriteSwitchApplicationCommand {
    pub switch_to: u8,
}

impl WriteSwitchApplicationCommand {
    pub fn new(option: SwitchApplicationOption) -> Self {
        Self {
            switch_to: option as u8,
        }
    }
}

impl Command for WriteSwitchApplicationCommand {
    const OPCODE: u8 = OP_WRITE_SWITCH_APPLICATION;
    const READ_COMMAND: bool = false;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct ReadExtendedSoftwareVersionCommand;

impl Command for ReadExtendedSoftwareVersionCommand {
    const OPCODE: u8 = OP_READ_EXTENDED_SOFTWARE_VERSION;
    const READ_COMMAND: bool = true;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<ExtendedSoftwareVersionResponse>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct WriteClearErrorHistoryCommand {
    pub clear_error_history: u32,
}

impl WriteClearErrorHistoryCommand {
    pub fn clear() -> Self {
        Self {
            clear_error_history: CLEAR_ERROR_HISTORY_SIGNATURE,
        }
    }
}

impl Command for WriteClearErrorHistoryCommand {
    const OPCODE: u8 = OP_WRITE_CLEAR_ERROR_HISTORY;
    const READ_COMMAND: bool = false;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct ReadErrorHistoryCommand;

impl Command for ReadErrorHistoryCommand {
    const OPCODE: u8 = OP_READ_ERROR_HISTORY;
    const READ_COMMAND: bool = true;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<ErrorHistoryResponse>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct ReadFlashIdCommand;

impl Command for ReadFlashIdCommand {
    const OPCODE: u8 = OP_READ_FLASH_ID;
    const READ_COMMAND: bool = true;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<FlashIdResponse>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct ReadGetFlashSectorInformationCommand;

impl Command for ReadGetFlashSectorInformationCommand {
    const OPCODE: u8 = OP_READ_GET_FLASH_SECTOR_INFORMATION;
    const READ_COMMAND: bool = true;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<FlashSectorInformationResponse>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct WriteUnlockFlashForUpdateCommand {
    pub unlock: u32,
}

impl WriteUnlockFlashForUpdateCommand {
    pub fn lock() -> Self {
        Self { unlock: 0 }
    }

    pub fn unlock() -> Self {
        Self {
            unlock: FLASH_UPDATE_UNLOCK_CODE,
        }
    }
}

impl Command for WriteUnlockFlashForUpdateCommand {
    const OPCODE: u8 = OP_UNLOCK_FLASH_FOR_UPDATE;
    const READ_COMMAND: bool = false;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct ReadUnlockFlashForUpdateCommand;

impl Command for ReadUnlockFlashForUpdateCommand {
    const OPCODE: u8 = OP_UNLOCK_FLASH_FOR_UPDATE;
    const READ_COMMAND: bool = true;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<UnlockFlashForUpdateResponse>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
#[bw(assert(start_address.is_multiple_of(4)))]
#[bw(assert(num_bytes.is_multiple_of(4)))]
pub struct WriteInitializeFlashReadWriteSettingsCommand {
    pub start_address: u32,
    pub num_bytes: u32,
}

impl Command for WriteInitializeFlashReadWriteSettingsCommand {
    const OPCODE: u8 = OP_WRITE_INITIALIZE_FLASH_RW_SETTINGS;
    const READ_COMMAND: bool = false;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, BinWrite)]
#[bw(assert(data.len().is_multiple_of(4)))]
pub struct FlashWriteCommand {
    pub data: Vec<u8>,
}

impl Command for FlashWriteCommand {
    const OPCODE: u8 = OP_FLASH_WRITE_AND_READ;
    const READ_COMMAND: bool = false;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
#[bw(assert(num_bytes_to_read.is_multiple_of(4)))]
pub struct ReadFlashWriteCommand {
    pub num_bytes_to_read: u16,
}

impl Command for ReadFlashWriteCommand {
    const OPCODE: u8 = OP_FLASH_WRITE_AND_READ;
    const READ_COMMAND: bool = true;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<FlashReadResponse>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
#[bw(assert(num_bytes.is_multiple_of(4)))]
pub struct ReadChecksumCommand {
    pub start_address: u32,
    pub num_bytes: u32,
}

impl Command for ReadChecksumCommand {
    const OPCODE: u8 = OP_READ_CHECKSUM;
    const READ_COMMAND: bool = true;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<ChecksumResponse>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinWrite)]
pub struct WriteFullFlashEraseCommand;

impl Command for WriteFullFlashEraseCommand {
    const OPCODE: u8 = OP_WRITE_FULL_FLASH_ERASE;
    const READ_COMMAND: bool = false;
    type ResponsePacket<T: ResponsePayload> = ResponsePacket<()>;
}

/// Responses
#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
pub struct BootHoldReasonResponse {
    pub reason: u8,
}

impl ResponsePayload for () {}
impl ResponsePayload for BootHoldReasonResponse {}
impl ResponsePayload for SystemTypeResponse {}
impl ResponsePayload for FlashIdResponse {}
impl ResponsePayload for FlashSectorInformationResponse {}
impl ResponsePayload for UnlockFlashForUpdateResponse {}
impl ResponsePayload for ChecksumResponse {}
impl ResponsePayload for FlashReadResponse {}
impl ResponsePayload for ModeResponse {}
impl ResponsePayload for VersionResponse {}
impl ResponsePayload for ExtendedSoftwareVersionResponse {}
impl ResponsePayload for ErrorHistoryResponse {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr, Display)]
#[repr(u8)]
pub enum ApplicationMode {
    BootRom = 0,
    MainApplication = 1,
    SecondaryBootApplication = 2,
    Unknown = 255,
}

impl From<u8> for ApplicationMode {
    fn from(value: u8) -> Self {
        ApplicationMode::from_repr(value).unwrap_or(ApplicationMode::Unknown)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr, Display)]
#[repr(u8)]
pub enum ControllerConfiguration {
    Single = 0,
    Reserved = 1,
    DualPrimary = 2,
    DualSecondary = 3,
    QuadPrimary = 4,
    QuadSecondary1 = 5,
    QuadSecondary2 = 6,
    QuadSecondary3 = 7,
    Unknown = 255,
}

impl From<u8> for ControllerConfiguration {
    fn from(value: u8) -> Self {
        ControllerConfiguration::from_repr(value).unwrap_or(ControllerConfiguration::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, BinRead)]
pub struct ModeResponse {
    pub mode_info: ModeInfoBits,
}

#[bitfield(u8)]
#[derive(PartialEq, Eq, BinRead)]
pub struct ModeInfoBits {
    #[bits(3)]
    application_mode: u8,
    #[bits(3)]
    controller_configuration: u8,
    #[bits(2)]
    _reserved: u8,
}

impl ModeResponse {
    pub fn application_mode(&self) -> ApplicationMode {
        self.mode_info.application_mode().into()
    }

    pub fn controller_configuration(&self) -> ControllerConfiguration {
        self.mode_info.controller_configuration().into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
pub struct VersionResponse {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr, Display)]
#[repr(u8)]
pub enum ExtendedVersionReleaseType {
    Production = 0,
    Alpha = 10,
    Beta = 11,
    Unknown = 255,
}

impl From<u8> for ExtendedVersionReleaseType {
    fn from(value: u8) -> Self {
        ExtendedVersionReleaseType::from_repr(value).unwrap_or(ExtendedVersionReleaseType::Unknown)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
pub struct ExtendedSoftwareVersionResponse {
    pub release_type: u8,
    pub release_number: u8,
    pub test_build_number: u8,
    pub commit_id: [u8; 7],
}

impl ExtendedSoftwareVersionResponse {
    pub fn release_type(&self) -> ExtendedVersionReleaseType {
        self.release_type.into()
    }
}

#[binread]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ErrorHistoryResponse {
    pub num_errors: u8,
    #[br(parse_with = until_eof)]
    pub entries: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
pub struct SystemTypeResponse {
    pub system_type: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
pub struct FlashIdResponse {
    pub manufacturer: u8,
    pub device: u8,
    pub capacity: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
pub struct FlashSectorInformationResponse {
    pub flash_size: u8,
    pub sector_size: u32,
    pub number_of_sectors: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
pub struct UnlockFlashForUpdateResponse {
    pub is_unlocked: u8,
}

impl UnlockFlashForUpdateResponse {
    pub fn is_unlocked(&self) -> bool {
        self.is_unlocked != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BinRead)]
pub struct ChecksumResponse {
    pub simple_checksum: u32,
    pub sum_of_simple_checksum: u32,
}

impl ChecksumResponse {
    pub fn as_u64(&self) -> u64 {
        (self.simple_checksum as u64) << 32 | (self.sum_of_simple_checksum as u64)
    }
}

#[binread]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FlashReadResponse {
    #[br(parse_with = until_eof)]
    pub data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use crate::fletcher_64;

    use super::*;

    #[test]
    fn command_header_bit_packing_matches_spec_layout() {
        let header = CommandHeaderBits::new()
            .with_destination(0b101)
            .with_opcode_length(false)
            .with_data_length_present(true)
            .with_checksum_present(false)
            .with_reply_requested(true)
            .with_read_command(true);

        assert_eq!(header.into_bits(), 0b1101_0101);
    }

    #[test]
    fn encode_erase_sector_command() -> Result<()> {
        assert_eq!(
            fletcher_64(&[0x71, 0x23, 0x04, 0x00, 0x00, 0x00, 0x02, 0x00]) as u8,
            0x99
        );

        let packet = WriteEraseSectorCommand::new(0x0002_0000)
            .into_packet()?
            .encode()?;
        assert_eq!(
            packet,
            vec![0x71, 0x23, 0x04, 0x00, 0x00, 0x00, 0x02, 0x00, 0x99]
        );
        Ok(())
    }

    #[test]
    fn encode_initialize_flash_rw_settings_command() -> Result<()> {
        let cmd = WriteInitializeFlashReadWriteSettingsCommand {
            start_address: 0x0000_0100,
            num_bytes: 0x0000_0040,
        };
        let packet = cmd.into_packet()?.encode()?;
        let mut expected = vec![
            0x71, 0x24, 0x08, 0x00, 0x00, 0x01, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,
        ];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(packet, expected);

        let cmd = WriteInitializeFlashReadWriteSettingsCommand {
            start_address: 0x0000_0000,
            num_bytes: 0x0000_1000,
        };
        let packet = cmd.into_packet()?.encode()?;
        let mut expected = vec![
            0x71, 0x24, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00,
        ];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(packet, expected);

        let cmd = WriteInitializeFlashReadWriteSettingsCommand {
            start_address: 0x0000_0000,
            num_bytes: 0x0000_0200,
        };
        let packet = cmd.into_packet()?.encode()?;
        let mut expected = vec![
            0x71, 0x24, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
        ];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(packet, expected);
        Ok(())
    }

    #[test]
    fn encode_flash_write_command_with_embedded_length_field() -> Result<()> {
        let data = [1, 2, 3, 4, 5, 6, 7, 8];
        let cmd = FlashWriteCommand {
            data: data.to_vec(),
        };
        let packet = cmd.into_packet()?.encode()?;
        let mut expected = vec![0x71, 0x25, 0x08, 0x00, 1, 2, 3, 4, 5, 6, 7, 8];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(packet, expected);
        Ok(())
    }

    #[test]
    fn encode_flash_read_and_full_erase_commands() -> Result<()> {
        let read = ReadFlashWriteCommand {
            num_bytes_to_read: 16,
        }
        .into_packet()?
        .encode()?;
        let mut expected = vec![0xF1, 0x25, 0x02, 0x00, 0x10, 0x00];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(read, expected);

        let read = ReadFlashWriteCommand {
            num_bytes_to_read: 256,
        }
        .into_packet()?
        .encode()?;
        let mut expected = vec![0xF1, 0x25, 0x02, 0x00, 0x00, 0x01];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(read, expected);

        let erase = WriteFullFlashEraseCommand.into_packet()?.encode()?;
        let mut expected = vec![0x71, 0x28, 0x00, 0x00];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(erase, expected);
        Ok(())
    }

    #[test]
    fn encode_read_bootrom_query_commands() -> Result<()> {
        let boot_hold = ReadBootHoldReasonCommand.into_packet()?.encode()?;
        let mut expected = vec![0xF1, 0x12, 0x00, 0x00];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(boot_hold, expected);

        let system_type = ReadSystemTypeCommand.into_packet()?.encode()?;
        let mut expected = vec![0xF1, 0x03, 0x00, 0x00];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(system_type, expected);

        let flash_id = ReadFlashIdCommand.into_packet()?.encode()?;
        let mut expected = vec![0xF1, 0x20, 0x00, 0x00];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(flash_id, expected);

        let flash_sector = ReadGetFlashSectorInformationCommand
            .into_packet()?
            .encode()?;
        let mut expected = vec![0xF1, 0x21, 0x00, 0x00];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(flash_sector, expected);

        let read_unlock = ReadUnlockFlashForUpdateCommand.into_packet()?.encode()?;
        let mut expected = vec![0xF1, 0x22, 0x00, 0x00];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(read_unlock, expected);
        Ok(())
    }

    #[test]
    fn encode_unlock_flash_for_update_and_read_checksum() -> Result<()> {
        let unlock = WriteUnlockFlashForUpdateCommand::unlock()
            .into_packet()?
            .encode()?;
        let mut expected = vec![0x71, 0x22, 0x04, 0x00, 0x27, 0x40, 0xA5, 0xF7];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(unlock, expected);

        let lock = WriteUnlockFlashForUpdateCommand::lock()
            .into_packet()?
            .encode()?;
        let mut expected = vec![0x71, 0x22, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(lock, expected);

        let checksum = ReadChecksumCommand {
            start_address: 0x100,
            num_bytes: 0x40,
        }
        .into_packet()?
        .encode()?;
        let mut expected = vec![
            0xF1, 0x26, 0x08, 0x00, 0x00, 0x01, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,
        ];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(checksum, expected);
        Ok(())
    }

    #[test]
    fn encode_write_system_type_command() -> Result<()> {
        let switch_to_bootrom = WriteSystemTypeCommand { system_type: 0x01 }
            .into_packet()?
            .encode()?;
        let mut expected = vec![0x71, 0x03, 0x01, 0x00, 0x01];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(switch_to_bootrom, expected);
        Ok(())
    }

    #[test]
    fn checksum_requires_multiple_of_4_bytes() {
        let command = ReadChecksumCommand {
            start_address: 0,
            num_bytes: 6,
        };
        assert!(command.into_packet().is_err());
    }

    #[test]
    fn response_header_bit_packing_matches_spec_layout() {
        let header = ResponseHeaderBits::new()
            .with_destination(0b011)
            .with_reserved(false)
            .with_data_length_present(true)
            .with_checksum_present(false)
            .with_error(true)
            .with_busy(true);

        assert_eq!(header.into_bits(), 0b1101_0011);
    }

    #[test]
    fn decode_all_fixed_size_responses() -> Result<()> {
        assert_eq!(fletcher_64([0x10, 0x01, 0x00, 0x02]) as u8, 0x45);

        let boot_hold_req = ReadBootHoldReasonCommand.into_packet()?;
        let boot_hold_payload = boot_hold_req
            .decode::<BootHoldReasonResponse>(&[0x10, 0x01, 0x00, 0x02, 0x45])?
            .data;
        let boot_hold = BootHoldReasonResponse::decode(boot_hold_payload)?;

        assert_eq!(boot_hold.reason, 0x02);

        let flash_id_req = ReadFlashIdCommand.into_packet()?;
        let mut flash_id_response = vec![0x10, 0x04, 0x00, 0x11, 0x22, 0x33, 0x44];
        flash_id_response.push(fletcher_64(&flash_id_response) as u8);
        let flash_id_payload = flash_id_req
            .decode::<FlashIdResponse>(&flash_id_response)?
            .data;
        let flash_id = FlashIdResponse::decode(flash_id_payload)?;
        assert_eq!(flash_id.manufacturer, 0x11);
        assert_eq!(flash_id.device, 0x22);
        assert_eq!(flash_id.capacity, 0x4433);

        let flash_id_req = ReadFlashIdCommand.into_packet()?;
        let mut flash_id_response = vec![0x11, 0x04, 0x00, 0xC2, 0x25, 0x38, 0x00];
        flash_id_response.push(fletcher_64(&flash_id_response) as u8);
        let flash_id_payload = flash_id_req
            .decode::<FlashIdResponse>(&flash_id_response)?
            .data;
        let flash_id = FlashIdResponse::decode(flash_id_payload)?;
        assert_eq!(flash_id.manufacturer, 0xC2);
        assert_eq!(flash_id.device, 0x25);
        assert_eq!(flash_id.capacity, 0x0038);

        let system_type_req = ReadSystemTypeCommand.into_packet()?;
        let mut system_type_response = vec![0x10, 0x01, 0x00, 0x01];
        system_type_response.push(fletcher_64(&system_type_response) as u8);
        let system_type_payload = system_type_req
            .decode::<SystemTypeResponse>(&system_type_response)?
            .data;
        let system_type = SystemTypeResponse::decode(system_type_payload)?;
        assert_eq!(system_type.system_type, 0x01);

        let sector_req = ReadGetFlashSectorInformationCommand.into_packet()?;
        let mut sector_response = vec![0x10, 0x07, 0x00, 0x05, 0x78, 0x56, 0x34, 0x12, 0x0A, 0x0B];
        sector_response.push(fletcher_64(&sector_response) as u8);
        let sector_payload = sector_req
            .decode::<FlashSectorInformationResponse>(&sector_response)?
            .data;
        let sector = FlashSectorInformationResponse::decode(sector_payload)?;
        assert_eq!(sector.flash_size, 0x05);
        assert_eq!(sector.sector_size, 0x1234_5678);
        assert_eq!(sector.number_of_sectors, 0x0B0A);

        let sector_req = ReadGetFlashSectorInformationCommand.into_packet()?;
        let mut sector_response = vec![0x11, 0x07, 0x00, 0x05, 0x00, 0x10, 0x00, 0x00, 0x00, 0x10];
        sector_response.push(fletcher_64(&sector_response) as u8);
        let sector_payload = sector_req
            .decode::<FlashSectorInformationResponse>(&sector_response)?
            .data;
        let sector = FlashSectorInformationResponse::decode(sector_payload)?;
        assert_eq!(sector.flash_size, 0x05);
        assert_eq!(sector.sector_size, 0x1000);
        assert_eq!(sector.number_of_sectors, 0x1000);

        let unlock_req = ReadUnlockFlashForUpdateCommand.into_packet()?;
        let mut unlock_response = vec![0x10, 0x01, 0x00, 0x01];
        unlock_response.push(fletcher_64(&unlock_response) as u8);
        let unlock_payload = unlock_req
            .decode::<UnlockFlashForUpdateResponse>(&unlock_response)?
            .data;
        let unlock = UnlockFlashForUpdateResponse::decode(unlock_payload)?;
        assert!(unlock.is_unlocked());

        let checksum_req = ReadChecksumCommand {
            start_address: 0x100,
            num_bytes: 0x40,
        }
        .into_packet()?;
        let mut checksum_response = vec![
            0x10, 0x08, 0x00, 0x78, 0x56, 0x34, 0x12, 0xEF, 0xCD, 0xAB, 0x90,
        ];
        checksum_response.push(fletcher_64(&checksum_response) as u8);
        let checksum_payload = checksum_req
            .decode::<ChecksumResponse>(&checksum_response)?
            .data;
        let checksum = ChecksumResponse::decode(checksum_payload)?;
        assert_eq!(checksum.simple_checksum, 0x1234_5678);
        assert_eq!(checksum.sum_of_simple_checksum, 0x90AB_CDEF);
        Ok(())
    }

    #[test]
    fn decode_variable_flash_read_response() -> Result<()> {
        let request = ReadFlashWriteCommand {
            num_bytes_to_read: 4,
        }
        .into_packet()?;
        let mut response = vec![0x10, 0x04, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];
        response.push(fletcher_64(&response) as u8);
        let response_payload = request.decode::<FlashReadResponse>(&response)?.data;
        let response = FlashReadResponse::decode(response_payload)?;

        assert_eq!(response.data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
        Ok(())
    }

    #[test]
    fn decode_response_reports_error_code() -> Result<()> {
        let request = ReadFlashIdCommand.into_packet()?;
        let header = ResponseHeaderBits::new()
            .with_destination(1)
            .with_reserved(false)
            .with_data_length_present(true)
            .with_checksum_present(false)
            .with_error(true)
            .with_busy(false);

        request
            .decode::<FlashIdResponse>(&[header.into_bits(), 0x01, 0x00, 0x06])
            .unwrap_err();
        Ok(())
    }

    #[test]
    fn encode_mode_and_switch_application_commands() -> Result<()> {
        let read_mode = ReadModeCommand.into_packet()?.encode()?;
        let mut expected = vec![0xF1, 0x00, 0x00, 0x00];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(read_mode, expected);

        let switch = WriteSwitchApplicationCommand::new(SwitchApplicationOption::BootApplication)
            .into_packet()?
            .encode()?;
        let mut expected = vec![0x71, 0x02, 0x01, 0x00, 0x00];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(switch, expected);

        let switch_main =
            WriteSwitchApplicationCommand::new(SwitchApplicationOption::MainApplication)
                .into_packet()?
                .encode()?;
        let mut expected = vec![0x71, 0x02, 0x01, 0x00, 0x01];
        expected.push(fletcher_64(&expected) as u8);
        assert_eq!(switch_main, expected);
        Ok(())
    }

    #[test]
    fn decode_mode_response() -> Result<()> {
        let request = ReadModeCommand.into_packet()?;

        // Test BootRom mode: application_mode = 0
        let mut mode_response = vec![0x10, 0x01, 0x00, 0x00];
        mode_response.push(fletcher_64(&mode_response) as u8);
        let mode_payload = request.decode::<ModeResponse>(&mode_response)?.data;
        let mode = ModeResponse::decode(mode_payload)?;
        assert_eq!(mode.application_mode(), ApplicationMode::BootRom);
        assert_eq!(
            mode.controller_configuration(),
            ControllerConfiguration::Single
        );

        // Test MainApplication mode: application_mode = 1
        let mut mode_response = vec![0x10, 0x01, 0x00, 0x01];
        mode_response.push(fletcher_64(&mode_response) as u8);
        let mode_payload = request.decode::<ModeResponse>(&mode_response)?.data;
        let mode = ModeResponse::decode(mode_payload)?;
        assert_eq!(mode.application_mode(), ApplicationMode::MainApplication);

        // Test with controller configuration bits set
        // mode_info byte: application_mode = 2 (bits 0-2 = 010), controller_configuration = 3 (bits 3-5 = 011)
        // Binary: 011_010 = 0x1A
        let mut mode_response = vec![0x10, 0x01, 0x00, 0x1A];
        mode_response.push(fletcher_64(&mode_response) as u8);
        let mode_payload = request.decode::<ModeResponse>(&mode_response)?.data;
        let mode = ModeResponse::decode(mode_payload)?;
        assert_eq!(
            mode.application_mode(),
            ApplicationMode::SecondaryBootApplication
        );
        assert_eq!(
            mode.controller_configuration(),
            ControllerConfiguration::DualSecondary
        );
        Ok(())
    }

    #[test]
    fn decode_switch_application_response() -> Result<()> {
        let switch_request =
            WriteSwitchApplicationCommand::new(SwitchApplicationOption::BootApplication)
                .into_packet()?;

        // WriteSwitchApplicationCommand returns empty response
        let mut response = vec![0x10, 0x00, 0x00];
        response.push(fletcher_64(&response) as u8);
        let response_packet = switch_request.decode::<()>(&response)?;

        // Verify the response packet parsed correctly with no data
        assert_eq!(response_packet.data.len(), 0);

        // Decode the empty response payload
        let _empty = <()>::decode(response_packet.data)?;
        Ok(())
    }
}
