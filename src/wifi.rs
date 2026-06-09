use embassy_net::{Runner, Stack, StackResources};
use esp_radio::wifi::{Config, Interfaces, WifiController, sta::StationConfig};
use log::{error, info, warn};
use static_cell::StaticCell;

const WIFI_SSID: Option<&str> = option_env!("WIFI_SSID");
const WIFI_PASSWORD: Option<&str> = option_env!("WIFI_PASSWORD");

static NET_RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();

pub type NetRunner = Runner<'static, esp_radio::wifi::Interface<'static>>;

#[embassy_executor::task]
pub async fn net_task(mut runner: NetRunner) -> ! {
    runner.run().await
}

pub async fn connect(
    spawner: embassy_executor::Spawner,
    controller: &mut WifiController<'static>,
    interfaces: Interfaces<'static>,
) -> Option<Stack<'static>> {
    let (ssid, password) = match (WIFI_SSID, WIFI_PASSWORD) {
        (Some(ssid), Some(password)) if !ssid.is_empty() => (ssid, password),
        _ => {
            warn!(
                "Wi-Fi credentials are not configured. Build with WIFI_SSID and WIFI_PASSWORD environment variables."
            );
            return None;
        }
    };

    info!("Connecting to Wi-Fi SSID: {ssid}");

    let station_config = Config::Station(
        StationConfig::default()
            .with_ssid(ssid)
            .with_password(password.into()),
    );

    controller
        .set_config(&station_config)
        .expect("Failed to configure Wi-Fi station");

    match controller.connect_async().await {
        Ok(info) => info!(
            "Connected to Wi-Fi: ssid={:?}, channel={}, aid={}",
            info.ssid, info.channel, info.aid
        ),
        Err(err) => {
            error!("Failed to connect to Wi-Fi: {err:?}");
            return None;
        }
    }

    let config = embassy_net::Config::dhcpv4(Default::default());
    let seed = 0x1234_5678;
    let (stack, runner) = embassy_net::new(
        interfaces.station,
        config,
        NET_RESOURCES.init(StackResources::new()),
        seed,
    );

    spawner.spawn(net_task(runner).expect("failed to create network task"));

    info!("Waiting for DHCP address...");
    stack.wait_config_up().await;

    if let Some(config) = stack.config_v4() {
        info!("IP address: {}", config.address.address());
    }

    Some(stack)
}

pub fn ip_string(stack: Option<&Stack<'static>>) -> heapless::String<32> {
    let mut out = heapless::String::new();
    match stack.and_then(Stack::config_v4) {
        Some(config) => {
            let _ = core::fmt::write(&mut out, format_args!("{}", config.address.address()));
        }
        None => {
            let _ = out.push_str("no ip");
        }
    }
    out
}
