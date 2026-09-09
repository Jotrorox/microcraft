use embassy_net::{Stack, tcp::TcpSocket};
use embassy_time::{Duration, Timer, with_timeout};
use heapless::String;
use log::{debug, error, info, warn};
use microcraft_protocol::{
    protocol::{self, Packet},
    session, status,
};
use static_cell::ConstStaticCell;

static RX_BUFFER: ConstStaticCell<[u8; 1024]> = ConstStaticCell::new([0; 1024]);
static TX_BUFFER: ConstStaticCell<[u8; 1024]> = ConstStaticCell::new([0; 1024]);
static PACKET: ConstStaticCell<Packet> = ConstStaticCell::new(Packet::new());
static RESPONSE: ConstStaticCell<Packet> = ConstStaticCell::new(Packet::new());

const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);

fn prepare_response(response: &mut Packet) {
    let mut json = String::<{ status::JSON_CAPACITY }>::new();
    status::status_json(
        status::MINECRAFT_VERSION,
        status::PROTOCOL_VERSION,
        &mut json,
    )
    .expect("validated status configuration must fit JSON buffer");
    protocol::make_string_packet(&json, response).expect("status response must fit packet buffer");
    info!("Minecraft status listener on port {}", status::SERVER_PORT);
}

#[embassy_executor::task]
pub async fn task(stack: Stack<'static>) -> ! {
    let rx = RX_BUFFER.take();
    let tx = TX_BUFFER.take();
    let packet = PACKET.take();
    let response = RESPONSE.take();
    prepare_response(response);

    loop {
        stack.wait_config_up().await;
        handle_client(stack, rx, tx, packet, response).await;
    }
}

async fn handle_client(
    stack: Stack<'static>,
    rx: &mut [u8; 1024],
    tx: &mut [u8; 1024],
    packet: &mut Packet,
    response: &Packet,
) {
    // A fresh socket cannot inherit FIN-WAIT-2 or a cancelled read from
    // the previous peer. Drop removes it from the stack before reuse.
    let mut socket = TcpSocket::new(stack, rx, tx);
    socket.set_timeout(Some(CLIENT_TIMEOUT));
    socket.set_nagle_enabled(false);
    if accept(&mut socket).await {
        serve_client(&mut socket, packet, response).await;
    }
}

async fn accept(socket: &mut TcpSocket<'_>) -> bool {
    if let Err(err) = socket.accept(status::SERVER_PORT).await {
        error!("Minecraft status accept failed: {err:?}");
        shutdown(socket, false).await;
        Timer::after(Duration::from_millis(100)).await;
        return false;
    }
    true
}

async fn serve_client(socket: &mut TcpSocket<'_>, packet: &mut Packet, response: &[u8]) {
    let result = with_timeout(
        CLIENT_TIMEOUT,
        session::handle_connection(socket, packet, response),
    )
    .await;
    let graceful = report_result(result);
    shutdown(socket, graceful).await;
}

fn report_result(result: Result<Result<(), protocol::Error>, embassy_time::TimeoutError>) -> bool {
    match result {
        Ok(Ok(())) => {
            debug!("Minecraft status connection completed");
            true
        }
        Ok(Err(err)) => {
            warn!("Minecraft status connection ended: {err:?}");
            false
        }
        Err(_) => {
            warn!("Minecraft status client exceeded total deadline");
            false
        }
    }
}

async fn shutdown(socket: &mut TcpSocket<'_>, graceful: bool) {
    if graceful {
        socket.close();
        let _ = with_timeout(CLOSE_TIMEOUT, socket.flush()).await;
    }
    // close/flush only completes our FIN; the peer may never send its FIN.
    // Abort both halves and give the runner a bounded opportunity to send RST.
    socket.abort();
    let _ = with_timeout(CLOSE_TIMEOUT, socket.flush()).await;
}
