use embedded_io_async::{Read, ReadExactError};
use heapless::Vec;

pub const MAX_PACKET_SIZE: usize = 512;
pub const MAX_STRING_SIZE: usize = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BufferTooSmall,
    InvalidVarInt,
    UnexpectedEof,
    ConnectionClosed,
    InvalidPacket,
    Io(embedded_io_async::ErrorKind),
}

pub struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    pub fn read_var_i32(&mut self) -> Result<i32, Error> {
        let mut value = 0i32;
        for byte_index in 0..5 {
            let byte = *self.bytes.get(self.pos).ok_or(Error::UnexpectedEof)?;
            self.pos += 1;
            if byte_index == 4 && byte & 0xf0 != 0 {
                return Err(Error::InvalidVarInt);
            }
            value |= ((byte & 0x7f) as i32) << (7 * byte_index);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(Error::InvalidVarInt)
    }

    pub fn read_u16(&mut self) -> Result<u16, Error> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_i64(&mut self) -> Result<i64, Error> {
        let bytes = self.read_bytes(8)?;
        Ok(i64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    pub fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], Error> {
        if self.remaining() < len {
            return Err(Error::UnexpectedEof);
        }
        let start = self.pos;
        self.pos += len;
        Ok(&self.bytes[start..start + len])
    }

    pub fn read_string(&mut self) -> Result<&'a str, Error> {
        let len = self.read_var_i32()?;
        if len < 0 || len as usize > MAX_STRING_SIZE {
            return Err(Error::InvalidPacket);
        }

        let bytes = self.read_bytes(len as usize)?;
        core::str::from_utf8(bytes).map_err(|_| Error::InvalidPacket)
    }
}

pub fn write_var_i32(mut value: i32, out: &mut Vec<u8, MAX_PACKET_SIZE>) -> Result<(), Error> {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value = ((value as u32) >> 7) as i32;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte).map_err(|_| Error::BufferTooSmall)?;
        if value == 0 {
            return Ok(());
        }
    }
}

/// Encode directly into caller-owned storage, including framing overhead.
pub fn make_packet(packet_id: i32, payload: &[u8], out: &mut Packet) -> Result<(), Error> {
    out.clear();
    let body_len = var_i32_len(packet_id) + payload.len();
    if body_len > MAX_PACKET_SIZE || body_len + var_i32_len(body_len as i32) > MAX_PACKET_SIZE {
        return Err(Error::BufferTooSmall);
    }
    write_var_i32(body_len as i32, out)?;
    write_var_i32(packet_id, out)?;
    out.extend_from_slice(payload)
        .map_err(|_| Error::BufferTooSmall)
}

pub type Packet = Vec<u8, MAX_PACKET_SIZE>;

pub fn var_i32_len(value: i32) -> usize {
    let mut value = value as u32;
    let mut len = 1;
    while value >= 128 {
        value >>= 7;
        len += 1;
    }
    len
}

pub fn make_string_packet(value: &str, out: &mut Packet) -> Result<(), Error> {
    out.clear();
    if value.len() > MAX_PACKET_SIZE {
        return Err(Error::BufferTooSmall);
    }
    let body_len = 1 + var_i32_len(value.len() as i32) + value.len();
    if body_len + var_i32_len(body_len as i32) > MAX_PACKET_SIZE {
        return Err(Error::BufferTooSmall);
    }
    write_var_i32(body_len as i32, out)?;
    write_var_i32(0, out)?;
    write_var_i32(value.len() as i32, out)?;
    out.extend_from_slice(value.as_bytes())
        .map_err(|_| Error::BufferTooSmall)
}

/// Read just the length prefix byte by byte, then read the body in chunks.
/// Cancellation consumes partial input; callers must discard the connection.
pub async fn read_packet(reader: &mut impl Read, packet: &mut Packet) -> Result<(), Error> {
    packet.clear();
    let mut prefix = [0; 5];
    let mut length = None;
    for i in 0..prefix.len() {
        if let Err(error) = reader.read_exact(&mut prefix[i..i + 1]).await {
            return Err(match error {
                ReadExactError::UnexpectedEof if i == 0 => Error::ConnectionClosed,
                error => read_error(error),
            });
        }
        if prefix[i] & 0x80 == 0 {
            length = Some(Cursor::new(&prefix[..=i]).read_var_i32()?);
            break;
        }
    }
    let length = length.ok_or(Error::InvalidVarInt)?;
    if length <= 0 || length as usize > MAX_PACKET_SIZE {
        return Err(Error::InvalidPacket);
    }
    packet
        .resize(length as usize, 0)
        .map_err(|_| Error::BufferTooSmall)?;
    reader
        .read_exact(packet.as_mut_slice())
        .await
        .map_err(read_error)
}

fn read_error<E: embedded_io_async::Error>(error: ReadExactError<E>) -> Error {
    match error {
        ReadExactError::UnexpectedEof => Error::UnexpectedEof,
        ReadExactError::Other(error) => Error::Io(error.kind()),
    }
}
