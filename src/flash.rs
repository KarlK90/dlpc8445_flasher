use std::{path::Path, time::Duration};

use log::{info, warn};

use crate::{Dlpc8445Error, Result, fletcher_64};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashState {
    pub current_sector: usize,
    pub sectors: Vec<FlashSector>,
}

// As per DLPC8445 datasheet (DLPS253C): "Table 6-11. Feature Requirements for
// Serial Flash Device Compatibility with DLPC8445"
pub const FLASH_PAGE_SIZE: usize = 256;
pub const FLASH_SECTOR_SIZE: usize = 4096;

// As per MX25U12835F datasheet (PM1728): "14. ERASE AND PROGRAMMING
// PERFORMANCE"
pub const FLASH_SECTOR_ERASE_TIME: Duration = Duration::from_millis(50); // Typical: 35ms, Max: 200ms
pub const FLASH_PAGE_PROGRAM_TIME: Duration = Duration::from_millis(3); // Typical: 0.5ms, Max: 3ms

impl FlashState {
    pub fn is_done(&self) -> bool {
        self.current_sector >= self.sectors.len()
    }

    pub fn advance_sector(&mut self) {
        self.current_sector += 1;
    }

    pub fn reset_current_sector(&mut self) {
        let sector = &mut self.sectors[self.current_sector];
        warn!(
            "Interrupted on sector {}; restarting from 0x{:08X}",
            sector.idx, sector.start_addr
        );
        sector.reset();
    }

    pub fn from_image(path: impl AsRef<Path>) -> Result<Self> {
        info!("Loading image from {}", path.as_ref().display());
        let image = std::fs::read(&path)?;

        if !image.len().is_multiple_of(4) {
            return Err(Dlpc8445Error::general(format!(
                "flash image size must be a multiple of 4 bytes, got {}",
                image.len()
            )));
        }
        info!(
            "Using DLPC Image: {} ({} bytes) checksum: {:#X}",
            path.as_ref().display(),
            image.len(),
            fletcher_64(&image)
        );

        let sectors = image
            .chunks(FLASH_SECTOR_SIZE)
            .enumerate()
            .map(|(idx, data)| FlashSector {
                idx,
                current_addr: 0,
                start_addr: idx * FLASH_SECTOR_SIZE,
                end_addr: idx * FLASH_SECTOR_SIZE + data.len(),
                verified: false,
                erased: false,
                data: data.to_vec(),
                checksum: fletcher_64(data),
            })
            .collect::<Vec<_>>();

        info!("Total flash sectors: {}", sectors.len());

        Ok(FlashState {
            current_sector: 0,
            sectors,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashSector {
    pub idx: usize,
    pub current_addr: usize,
    pub start_addr: usize,
    pub end_addr: usize,
    pub verified: bool,
    pub erased: bool,
    pub data: Vec<u8>,
    pub checksum: u64,
}

impl FlashSector {
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn remaining(&self) -> usize {
        self.len().saturating_sub(self.current_addr)
    }

    pub fn is_programmed(&self) -> bool {
        self.current_addr == self.len()
    }

    pub fn reset(&mut self) {
        self.current_addr = 0;
        self.verified = false;
        self.erased = false;
    }

    pub fn mark_erased(&mut self) {
        self.current_addr = 0;
        self.verified = false;
        self.erased = true;
    }
}
