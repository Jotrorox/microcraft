#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

#[path = "../drawing.rs"]
mod drawing;
#[path = "../minecraft/mod.rs"]
mod minecraft;
#[path = "../net/mod.rs"]
mod net;
#[path = "../oled.rs"]
mod oled;

use drawing::ResourceUsage;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use log::info;

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Seeed XIAO ESP32S3: D4 = SDA (GPIO5), D5 = SCL (GPIO6).
    let mut display = oled::new(peripherals.I2C0, peripherals.GPIO5, peripherals.GPIO6);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialized!");

    let (mut wifi_controller, interfaces) =
        esp_radio::wifi::new(peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

    let stack = net::wifi::connect(spawner, &mut wifi_controller, interfaces).await;

    if let Some(stack) = stack {
        spawner.spawn(
            minecraft::server::task(stack).expect("failed to create Minecraft status server task"),
        );
    }

    let mut status_ticks = 0u32;

    loop {
        let ip = net::wifi::ip_string(stack.as_ref());
        let usage = ResourceUsage::current();

        // Avoid flooding the serial log; the OLED still refreshes regularly.
        if status_ticks % 5 == 0 {
            info!(
                "IP address: {}, heap used={} B, heap free={} B, heap used={}%,",
                ip,
                usage.heap_used,
                usage.heap_free,
                usage.heap_percent()
            );
        }
        status_ticks = status_ticks.wrapping_add(1);

        if let Some(display) = display.as_mut() {
            drawing::draw_status(display, &ip, &usage).ok();
            display.flush().ok();
        }

        Timer::after(Duration::from_secs(2)).await;
    }
}
