use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::i2c::master::{Config, I2c};
use esp_hal::peripherals::I2C0;
use log::{error, info};
use ssd1306::mode::BufferedGraphicsModeAsync;
use ssd1306::prelude::*;
use ssd1306::{I2CDisplayInterface, Ssd1306Async};

pub type Display = Ssd1306Async<
    I2CInterface<I2c<'static, esp_hal::Async>>,
    DisplaySize128x64,
    BufferedGraphicsModeAsync<DisplaySize128x64>,
>;

#[allow(
    clippy::large_stack_frames,
    reason = "one-time construction of the fixed 1 KiB OLED framebuffer on the startup stack"
)]
pub fn new(
    i2c0: I2C0<'static>,
    sda: impl PeripheralInput<'static> + PeripheralOutput<'static>,
    scl: impl PeripheralInput<'static> + PeripheralOutput<'static>,
) -> Display {
    let i2c = I2c::new(i2c0, Config::default())
        .expect("failed to initialize I2C")
        .with_sda(sda)
        .with_scl(scl)
        .into_async();

    let interface = I2CDisplayInterface::new(i2c);
    Ssd1306Async::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode()
}

#[allow(
    clippy::large_stack_frames,
    reason = "one-time SSD1306 initialization uses fixed-size driver futures on the startup stack"
)]
pub async fn initialize(display: &mut Display) -> bool {
    match display.init().await {
        Ok(()) => {
            display.clear_buffer();
            if let Err(err) = display.flush().await {
                error!("initial OLED flush failed: {err:?}");
                return false;
            }
            info!("SSD1306 display initialized at default I2C address 0x3c");
            true
        }
        Err(err) => {
            error!("failed to initialize SSD1306 display: {err:?}");
            false
        }
    }
}
