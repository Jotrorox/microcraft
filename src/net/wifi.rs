use embassy_net::{Runner, Stack, StackResources};
use embassy_time::{Duration, Timer, with_timeout};
use esp_hal::rng::Rng;
use esp_radio::wifi::{
    AuthenticationMethod, Config, Interfaces, WifiController, sta::StationConfig,
};
use log::{error, info, warn};
use static_cell::ConstStaticCell;

const WIFI_SSID: Option<&str> = option_env!("WIFI_SSID");
const WIFI_PASSWORD: Option<&str> = option_env!("WIFI_PASSWORD");

// One DHCP socket and one status socket.
static NET_RESOURCES: ConstStaticCell<StackResources<2>> =
    ConstStaticCell::new(StackResources::new());

pub type NetRunner = Runner<'static, esp_radio::wifi::Interface<'static>>;

#[embassy_executor::task]
pub async fn net_task(mut runner: NetRunner) -> ! {
    runner.run().await
}

pub fn start(
    spawner: embassy_executor::Spawner,
    mut controller: WifiController<'static>,
    interfaces: Interfaces<'static>,
) -> Option<Stack<'static>> {
    let (ssid, password) = match (WIFI_SSID, WIFI_PASSWORD) {
        (Some(ssid), Some(password)) if !ssid.is_empty() => (ssid, password),
        _ => {
            warn!("Wi-Fi credentials are not configured; networking disabled");
            return None;
        }
    };
    if ssid.len() > 32 || password.len() > 64 {
        error!("Wi-Fi SSID/password exceeds the supported byte length");
        return None;
    }
    if let Err(err) = configure(&mut controller, ssid, password) {
        error!("Failed to configure Wi-Fi station: {err:?}");
        return None;
    }

    // set_config has enabled the RF subsystem, which supplies RNG entropy.
    let rng = Rng::new();
    let seed = (u64::from(rng.random()) << 32) | u64::from(rng.random());
    let (stack, runner) = embassy_net::new(
        interfaces.station,
        embassy_net::Config::dhcpv4(Default::default()),
        NET_RESOURCES.take(),
        seed,
    );
    spawner.spawn(net_task(runner).expect("failed to create network task"));
    spawner.spawn(connection_task(controller, stack).expect("failed to create Wi-Fi task"));
    Some(stack)
}

fn configure(
    controller: &mut WifiController<'static>,
    ssid: &str,
    password: &str,
) -> Result<(), esp_radio::wifi::WifiError> {
    let mut station = StationConfig::default()
        .with_ssid(ssid)
        .with_password(password.into());
    if password.is_empty() {
        station = station.with_auth_method(AuthenticationMethod::None);
    }
    controller.set_config(&Config::Station(station))
}

#[embassy_executor::task]
async fn connection_task(mut controller: WifiController<'static>, stack: Stack<'static>) -> ! {
    let mut retry_seconds = 1;
    loop {
        if connect(&mut controller).await {
            retry_seconds = 1;
            wait_for_disconnect(&controller, stack).await;
            warn!("Wi-Fi disconnected; reconnecting");
        }
        Timer::after(Duration::from_secs(retry_seconds)).await;
        retry_seconds = (retry_seconds * 2).min(30);
    }
}

async fn connect(controller: &mut WifiController<'static>) -> bool {
    info!("Connecting to Wi-Fi");
    match controller.connect_async().await {
        Ok(_) => {
            info!("Wi-Fi connected; waiting for DHCP");
            true
        }
        Err(err) => {
            warn!("Wi-Fi connection failed: {err:?}");
            false
        }
    }
}

async fn wait_for_disconnect(controller: &WifiController<'static>, stack: Stack<'static>) {
    // DHCP retries internally. A bounded wait lets us report stalled address
    // acquisition and check for a disconnected radio even before DHCP succeeds.
    while controller.is_connected() {
        if with_timeout(Duration::from_secs(30), stack.wait_config_up())
            .await
            .is_ok()
        {
            if let Some(config) = stack.config_v4() {
                info!("IP address: {}", config.address.address());
            }
            let _ = controller.wait_for_disconnect_async().await;
            break;
        }
        warn!("DHCP not ready after 30 seconds; still waiting");
    }
}

pub fn ip_string(stack: Option<&Stack<'static>>) -> heapless::String<32> {
    let mut out = heapless::String::new();
    match stack {
        Some(stack) if !stack.is_link_up() => out.push_str("connecting").unwrap(),
        Some(stack) => match stack.config_v4() {
            Some(config) => {
                core::fmt::write(&mut out, format_args!("{}", config.address.address())).unwrap();
            }
            None => out.push_str("waiting DHCP").unwrap(),
        },
        None => out.push_str("wifi disabled").unwrap(),
    }
    out
}
