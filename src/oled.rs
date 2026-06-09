use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::i2c::master::{Config, I2c};
use esp_hal::peripherals::I2C0;
use log::{error, info};
use ssd1306::mode::BufferedGraphicsMode;
use ssd1306::prelude::*;
use ssd1306::{I2CDisplayInterface, Ssd1306};

pub type Display = Ssd1306<
    I2CInterface<I2c<'static, esp_hal::Blocking>>,
    DisplaySize128x64,
    BufferedGraphicsMode<DisplaySize128x64>,
>;

pub fn new(
    i2c0: I2C0<'static>,
    sda: impl PeripheralInput<'static> + PeripheralOutput<'static>,
    scl: impl PeripheralInput<'static> + PeripheralOutput<'static>,
) -> Option<Display> {
    let i2c = I2c::new(i2c0, Config::default())
        .expect("failed to initialize I2C")
        .with_sda(sda)
        .with_scl(scl);

    let interface = I2CDisplayInterface::new(i2c);
    let mut display = Ssd1306::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    match display.init() {
        Ok(()) => {
            info!("SSD1306 display initialized at default I2C address 0x3c");
            display.clear_buffer();
            display.flush().ok();
            Some(display)
        }
        Err(error) => {
            error!("failed to initialize SSD1306 display: {:?}", error);
            None
        }
    }
}
