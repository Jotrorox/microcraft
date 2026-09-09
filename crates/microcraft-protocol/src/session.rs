use embedded_io_async::{Error as _, Read, Write};

use crate::protocol::{self, Cursor, Error, Packet};

/// One status exchange. The transport owner supplies a total deadline and
/// closes the connection, including when this future is cancelled mid-packet.
pub async fn handle_connection(
    socket: &mut (impl Read + Write),
    packet: &mut Packet,
    response: &[u8],
) -> Result<(), Error> {
    protocol::read_packet(socket, packet).await?;
    validate_handshake(packet)?;

    protocol::read_packet(socket, packet).await?;
    validate_status_request(packet)?;
    send(socket, response).await?;

    // A status-only client may close without requesting a ping.
    match protocol::read_packet(socket, packet).await {
        Err(Error::ConnectionClosed) => return Ok(()),
        result => result?,
    }
    make_pong(packet)?;
    send(socket, packet).await
}

fn validate_status_request(packet: &[u8]) -> Result<(), Error> {
    let mut cursor = Cursor::new(packet);
    if cursor.read_var_i32()? != 0 || cursor.remaining() != 0 {
        return Err(Error::InvalidPacket);
    }
    Ok(())
}

fn make_pong(packet: &mut Packet) -> Result<(), Error> {
    let mut cursor = Cursor::new(packet);
    if cursor.read_var_i32()? != 1 {
        return Err(Error::InvalidPacket);
    }
    let payload = cursor.read_i64()?.to_be_bytes();
    if cursor.remaining() != 0 {
        return Err(Error::InvalidPacket);
    }
    protocol::make_packet(1, &payload, packet)
}

pub fn validate_handshake(packet: &[u8]) -> Result<(), Error> {
    let mut cursor = Cursor::new(packet);
    if cursor.read_var_i32()? != 0 {
        return Err(Error::InvalidPacket);
    }
    cursor.read_var_i32()?; // Any client version can request status.
    cursor.read_string()?;
    cursor.read_u16()?;
    if cursor.read_var_i32()? != 1 || cursor.remaining() != 0 {
        return Err(Error::InvalidPacket);
    }
    Ok(())
}

async fn send(socket: &mut impl Write, mut bytes: &[u8]) -> Result<(), Error> {
    while !bytes.is_empty() {
        let count = socket.write(bytes).await.map_err(|e| Error::Io(e.kind()))?;
        if count == 0 {
            return Err(Error::ConnectionClosed);
        }
        bytes = &bytes[count..];
    }
    socket.flush().await.map_err(|e| Error::Io(e.kind()))
}
