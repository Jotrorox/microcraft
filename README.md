# Microcraft on Seeed Studio XIAO ESP32S3

Rust `no_std` firmware for the **Seeed Studio XIAO ESP32S3**, using
`esp-hal`, Embassy, and Wi-Fi. It runs a Minecraft Java Edition status/ping
listener on TCP port 25565 and shows its IP address and heap usage on an
optional SSD1306 128×64 I2C OLED. It does not implement Minecraft gameplay.

## Toolchain setup

The ESP32-S3 uses Xtensa cores and needs Espressif's Rust toolchain.
`rustup target add` alone cannot install this toolchain. Install it with
[`espup`](https://github.com/esp-rs/espup), which registers the `esp` toolchain
with rustup:

```sh
rustup toolchain install stable --component rustfmt
cargo +stable install espup --locked
espup install --targets esp32s3
cargo +stable install espflash --locked
```

If building the installer needs unavailable system dependencies, use the
prebuilt executable for your platform from the
[espup releases](https://github.com/esp-rs/espup/releases).

On Linux/macOS, load the toolchain environment in each new terminal before
building (or add this line to your shell configuration):

```sh
source "$HOME/export-esp.sh"
```

The repository selects `esp` automatically through `rust-toolchain.toml` and
builds for `xtensa-esp32s3-none-elf`. No ESP-IDF installation is needed.

## OLED wiring

Connect the SSD1306 at its default I2C address, `0x3c`:

| OLED | XIAO ESP32S3 |
| --- | --- |
| VCC | 3V3 |
| GND | GND |
| SDA | D4 / GPIO5 |
| SCL | D5 / GPIO6 |

These are the board's [documented I2C pins](https://wiki.seeedstudio.com/xiao_esp32s3_getting_started/).
The firmware continues without the OLED if display initialization fails.
It uses internal RAM; external PSRAM, the Sense camera, and the SD card are
not initialized.

## Build and flash

Attach the supplied Wi-Fi antenna and connect the board over USB. Set your
2.4 GHz Wi-Fi credentials in the build environment:

```sh
export WIFI_SSID='your-network'
export WIFI_PASSWORD='your-password'
cargo build --release --locked
cargo run --release --locked
```

`cargo run` flashes the board with `espflash` and opens the serial monitor.
If necessary, select the serial port explicitly:

```sh
cargo run --release --locked -- --port /dev/ttyACM0
```

If the board does not enter the bootloader automatically, hold BOOT, press
and release RESET, then release BOOT and retry. On Linux your user also needs
read/write access to the serial device.

Credentials are compiled into the firmware. Without them the firmware still
builds and runs, but skips Wi-Fi connection and the Minecraft listener.
Once connected, add the displayed IP address (port 25565) to the Minecraft
Java Edition multiplayer server list. The advertised version/protocol can be
overridden with `MINECRAFT_VERSION` and `MINECRAFT_PROTOCOL_VERSION` at build
time.

## Checks

```sh
cargo +stable fmt --all -- --check
cargo build --release --locked
cargo clippy --all-features --workspace --locked -- -D warnings
```
