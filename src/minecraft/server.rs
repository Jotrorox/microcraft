use embassy_net::{Stack, tcp::TcpSocket};
use embassy_time::Duration;
use heapless::Vec;
use log::{debug, error, info, warn};
use static_cell::StaticCell;

use super::{protocol, status};

static RX_BUFFER: StaticCell<[u8; 1024]> = StaticCell::new();
static TX_BUFFER: StaticCell<[u8; 1024]> = StaticCell::new();

#[embassy_executor::task]
pub async fn task(stack: Stack<'static>) -> ! {
    let rx_buffer = RX_BUFFER.init([0; 1024]);
    let tx_buffer = TX_BUFFER.init([0; 1024]);
    let mut socket = TcpSocket::new(stack, rx_buffer, tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(10)));
    socket.set_nagle_enabled(false);

    loop {
        info!(
            "Minecraft status listener waiting on port {}",
            status::SERVER_PORT
        );
        if let Err(err) = socket.accept(status::SERVER_PORT).await {
            error!("Minecraft status accept failed: {err:?}");
            continue;
        }

        debug!("Minecraft status client connected");

        match handle_connection(&mut socket).await {
            Ok(()) => debug!("Minecraft status connection completed cleanly"),
            Err(err) => warn!("Minecraft status connection ended: {err:?}"),
        }

        debug!("Minecraft status closing client socket");
        let _ = socket.flush().await;
        socket.close();
        let _ = socket.flush().await;
    }
}

async fn handle_connection(socket: &mut TcpSocket<'_>) -> Result<(), protocol::Error> {
    let handshake = read_packet(socket).await?;
    debug!("Minecraft handshake packet: {} B", handshake.len());

    let mut cursor = protocol::Cursor::new(&handshake);
    let packet_id = cursor.read_var_i32()?;
    if packet_id != 0 {
        debug!("Unexpected handshake packet id: {packet_id}");
        return Err(protocol::Error::InvalidPacket);
    }

    let client_protocol = cursor.read_var_i32()?;
    let server_address = cursor.read_string()?;
    let server_port = cursor.read_u16()?;
    let next_state = cursor.read_var_i32()?;
    debug!(
        "Handshake: client_protocol={}, requested={}:{}, next_state={}, consumed={} B/{} B",
        client_protocol,
        server_address.as_str(),
        server_port,
        next_state,
        cursor.position(),
        handshake.len()
    );
    if client_protocol != status::PROTOCOL_VERSION {
        debug!(
            "Client protocol {} differs from advertised {} ({})",
            client_protocol,
            status::PROTOCOL_VERSION,
            status::MINECRAFT_VERSION
        );
    }
    if next_state != 1 {
        debug!("Unsupported next state: {next_state}");
        return Err(protocol::Error::InvalidPacket);
    }

    let request = read_packet(socket).await?;
    debug!("Minecraft status request packet: {} B", request.len());
    let mut cursor = protocol::Cursor::new(&request);
    let packet_id = cursor.read_var_i32()?;
    if packet_id != 0 {
        debug!("Unexpected status request packet id: {packet_id}");
        return Err(protocol::Error::InvalidPacket);
    }

    let json = status::status_json();
    debug!("Sending status response: {} B JSON", json.len());
    let payload = protocol::string_payload(&json)?;
    let response = protocol::make_packet(0, &payload)?;
    debug!("Status response packet: {} B", response.len());
    write_all(socket, &response).await?;

    let ping = read_packet(socket).await?;
    debug!("Minecraft ping packet: {} B", ping.len());
    let mut cursor = protocol::Cursor::new(&ping);
    let packet_id = cursor.read_var_i32()?;
    if packet_id != 1 {
        debug!("No ping response sent for packet id: {packet_id}");
        return Ok(());
    }
    let payload = cursor.read_i64()?.to_be_bytes();
    let response = protocol::make_packet(1, &payload)?;
    debug!("Sending pong packet: {} B", response.len());
    write_all(socket, &response).await?;
    Ok(())
}

async fn read_packet(
    socket: &mut TcpSocket<'_>,
) -> Result<Vec<u8, { protocol::MAX_PACKET_SIZE }>, protocol::Error> {
    let length = read_var_i32_from_socket(socket).await?;
    debug!("Incoming Minecraft packet length: {length} B");
    if length < 0 || length as usize > protocol::MAX_PACKET_SIZE {
        debug!(
            "Rejecting Minecraft packet with invalid length {} (max {})",
            length,
            protocol::MAX_PACKET_SIZE
        );
        return Err(protocol::Error::InvalidPacket);
    }

    let mut packet = Vec::<u8, { protocol::MAX_PACKET_SIZE }>::new();
    for _ in 0..length {
        let mut byte = [0u8; 1];
        read_exact(socket, &mut byte).await?;
        packet
            .push(byte[0])
            .map_err(|_| protocol::Error::BufferTooSmall)?;
    }
    Ok(packet)
}

async fn read_var_i32_from_socket(socket: &mut TcpSocket<'_>) -> Result<i32, protocol::Error> {
    let mut value = 0i32;
    for byte_index in 0..5 {
        let mut byte = [0u8; 1];
        read_exact(socket, &mut byte).await?;
        value |= ((byte[0] & 0x7f) as i32) << (7 * byte_index);
        if byte[0] & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(protocol::Error::InvalidVarInt)
}

async fn read_exact(socket: &mut TcpSocket<'_>, buf: &mut [u8]) -> Result<(), protocol::Error> {
    let mut offset = 0;
    while offset < buf.len() {
        let read = socket
            .read(&mut buf[offset..])
            .await
            .map_err(|_| protocol::Error::UnexpectedEof)?;
        if read == 0 {
            return Err(protocol::Error::UnexpectedEof);
        }
        offset += read;
    }
    Ok(())
}

async fn write_all(socket: &mut TcpSocket<'_>, buf: &[u8]) -> Result<(), protocol::Error> {
    let mut offset = 0;
    while offset < buf.len() {
        let written = socket
            .write(&buf[offset..])
            .await
            .map_err(|_| protocol::Error::UnexpectedEof)?;
        if written == 0 {
            return Err(protocol::Error::UnexpectedEof);
        }
        offset += written;
    }
    socket
        .flush()
        .await
        .map_err(|_| protocol::Error::UnexpectedEof)
}
