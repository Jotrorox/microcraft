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
Display transfers use async I2C, and unchanged status values skip the refresh.
Three consecutive transfer failures disable the display until the next boot.
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

The version must contain 1–64 UTF-8 bytes (at most 128 bytes after JSON
escaping), and the protocol must be an integer from 0 to 2147483647. Invalid
overrides fail compilation. Quotes, backslashes, and control characters in
version strings are escaped in the status response.

Wi-Fi connects in the background and retries failures with a 1–30 second
exponential backoff. After a disconnect it reconnects automatically. While
DHCP retries, the OLED continues to show connection status; a missing lease
is logged every 30 seconds. If Wi-Fi drops during DHCP acquisition, recovery
can take up to that interval. An empty `WIFI_PASSWORD` selects an open network.

The listener serves one client at a time, with a ten-second total exchange
deadline and up to two seconds of bounded socket cleanup. Slow clients cannot
extend the exchange by sending occasional bytes. The response is prepared
once, and fixed packet buffers are reused without heap allocation.

## Checks

```sh
cargo +stable fmt --all -- --check
cargo +stable test -p microcraft-protocol --target x86_64-unknown-linux-gnu --locked
python3 scripts/check_config.py
cargo build --release --locked
cargo clippy --all-features --workspace --locked -- -D warnings
```

The `microcraft-protocol` workspace crate runs on the host without the Espressif
toolchain. On another host architecture, replace `x86_64-unknown-linux-gnu`
with the host triple from `rustc +stable -vV`. Its regression tests cover TCP
fragmentation/coalescing, packet limits, malformed input, status-only clients,
exact ping echoes, transport failures, and JSON serialization. CI runs these
tests and checks that invalid build-time overrides fail compilation.

Firmware retains the 1 KiB Clippy stack-frame threshold. Packet and network
storage are static; only main and one-time OLED construction/initialization
have documented exceptions for fixed framebuffer and driver state. Clippy
estimates do not replace measuring stack high-water usage on the board.

After flashing, exercise repeated server-list refreshes, a client that leaves
its side open after receiving a pong, a slow/incomplete handshake, an AP
restart, missing DHCP, and an absent/disconnected OLED. These need real
hardware; host protocol tests do not validate the radio or I2C driver.
