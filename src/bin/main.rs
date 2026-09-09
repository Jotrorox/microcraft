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
use log::{info, warn};

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

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 65536);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    info!("Embassy initialized!");

    let (wifi_controller, interfaces) = esp_radio::wifi::new(peripherals.WIFI, Default::default())
        .expect("Failed to initialize Wi-Fi controller");

    let stack = net::wifi::start(spawner, wifi_controller, interfaces);

    if let Some(stack) = stack {
        spawner.spawn(
            minecraft::server::task(stack).expect("failed to create Minecraft status server task"),
        );
    }

    // Seeed XIAO ESP32S3: D4 = SDA (GPIO5), D5 = SCL (GPIO6).
    let mut display = oled::new(peripherals.I2C0, peripherals.GPIO5, peripherals.GPIO6);
    let mut display = if oled::initialize(&mut display).await {
        Some(display)
    } else {
        None
    };
    let mut previous_status = None;
    let mut display_failures = 0;
    let mut status_ticks = 0u32;

    loop {
        let ip = net::wifi::ip_string(stack.as_ref());
        let usage = ResourceUsage::current();

        // Avoid flooding the serial log; the OLED still refreshes regularly.
        if status_ticks.is_multiple_of(5) {
            info!(
                "IP address: {}, heap used={} B, heap free={} B, heap used={}%,",
                ip,
                usage.heap_used,
                usage.heap_free,
                usage.heap_percent()
            );
        }
        status_ticks = status_ticks.wrapping_add(1);

        let changed = previous_status
            .as_ref()
            .is_none_or(|(old_ip, old_usage)| old_ip != &ip || old_usage != &usage);
        if changed && let Some(oled) = display.as_mut() {
            // Drawing only changes RAM; the async transfer yields to networking.
            drawing::draw_status(oled, &ip, &usage).expect("framebuffer drawing is infallible");
            match oled.flush().await {
                Ok(()) => {
                    previous_status = Some((ip, usage));
                    display_failures = 0;
                }
                Err(err) => {
                    display_failures += 1;
                    warn!("OLED refresh failed ({display_failures}/3): {err:?}");
                    if display_failures >= 3 {
                        warn!("Disabling OLED after repeated transfer failures");
                        display = None;
                    }
                }
            }
        }

        Timer::after(Duration::from_secs(2)).await;
    }
}
