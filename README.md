# DLPC8445 Flash Programmer

USB flash programmer for the Texas Instruments DLP Controller (DLPC8445), enabling reliable firmware updates and validation:

- **Program Flash Memory**: Upload firmware images to the DLPC8445 flash memory via USB
- **Validate Flash Content**: Verify flash contents match the firmware image
- **Erase Flash Content**: Force erase all sectors before flashing
- **USB Resilience**: Automatically resume from connection interruptions without data loss

## How to Use

> [!CAUTION]
> **Only tested on Linux, but should work on macOS and Windows with USB support.**  
> **Use at your own risk:** Flashing firmware can permanently brick your device. You are fully responsible for any damage or data loss.

### Building from Source

Install Rust and Cargo (e.g. via [rustup](https://rustup.rs/)), then clone the repository and build the binary:

```bash
# Clone the repository
git clone <repository-url>
cd dlpc8445_flasher

# Build the binary
cargo build --release

# The binary will be available at:
# target/release/dlpc8445_flasher
```

### Command-Line Interface

#### Basic Help

```bash
./dlpc8445_flasher --help
```

#### Operation Modes

##### 1. Validation Mode (Default)

Verify the flash contains the expected firmware without writing:

```bash
# Validate with default image name (AWOL_DLP_Upgrade.img)
./dlpc8445_flasher

# Validate with custom image file
./dlpc8445_flasher --file /path/to/firmware.img
```

**Output**: Compares each sector's checksum with the image. Reports any mismatches.

##### 2. Flash Programming

Write firmware to the device:

```bash
# Program flash with default image
./dlpc8445_flasher --flash --file firmware.img

# Program with automatic mode switching
./dlpc8445_flasher --flash --file firmware.img --enter-flash-mode
```

> [!WARNING]
> `--enter-flash-mode` switches the DLPC8445 from application mode to bootrom.  
> This invalidates the image currently on flash, so you must flash a valid image immediately afterward.

**Behavior**:

- Connects to device and verifies flash mode (optionally switches with `--enter-flash-mode`)
- Invalidates header sector of the flash image
- Iterates through image sectors in reverse:
  - Skips sectors that already match the image (checksum validation)
  - Erases mismatched sectors
  - Flashes new data
  - Validates the written data

##### 3. Flash Erase

Force erase all flash sectors occupied by the image before flashing:

```bash
./dlpc8445_flasher --file firmware.img --erase
```

**Behavior**:

- Erases all flash sectors that would be occupied by the image

### CLI Flag Reference

| Flag                 | Short | Type | Description                                                   |
| -------------------- | ----- | ---- | ------------------------------------------------------------- |
| `--file`             | `-f`  | PATH | Firmware image file (default: `AWOL_DLP_Upgrade.img`)         |
| `--flash`            |       | FLAG | Enable flash programming (default: validation only)           |
| `--enter-flash-mode` |       | FLAG | Switch to flash mode; prompts before invalidating the current flash image |
| `--erase`            |       | FLAG | Erase all flash sectors that would be occupied by the image   |
| `--help`             | `-h`  |      | Display help message                                          |
| `--version`          | `-V`  |      | Display version information                                   |

### Error Recovery

The tool handles several error conditions:

1. **USB Disconnection**
   - Automatically resumes from the last completed sector

2. **Device Mode Issues**
   - Reports if device is in wrong mode
   - Suggests `--enter-flash-mode` flag
   - Verifies mode switch completion

## Datasheet References

This implementation follows the official Texas Instruments documentation:

- **[DLPU114A](https://www.ti.com/lit/pdf/DLPU114)**: Programmer's Guide DLPC8445 and DLPC8445V
- **[DLPS253](https://www.ti.com/lit/pdf/DLPS253)**: Datasheet DLPC8445, DLPC8445V, and DLPC8455 High-Resolution Controllers
