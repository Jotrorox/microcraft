use heapless::{String, Vec};

pub const MAX_PACKET_SIZE: usize = 512;
pub const MAX_STRING_SIZE: usize = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    BufferTooSmall,
    InvalidVarInt,
    UnexpectedEof,
    InvalidPacket,
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

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn read_string(&mut self) -> Result<String<MAX_STRING_SIZE>, Error> {
        let len = self.read_var_i32()?;
        if len < 0 || len as usize > MAX_STRING_SIZE {
            return Err(Error::InvalidPacket);
        }

        let bytes = self.read_bytes(len as usize)?;
        let value = core::str::from_utf8(bytes).map_err(|_| Error::InvalidPacket)?;
        String::try_from(value).map_err(|_| Error::BufferTooSmall)
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

pub fn make_packet(packet_id: i32, payload: &[u8]) -> Result<Vec<u8, MAX_PACKET_SIZE>, Error> {
    let mut body = Vec::<u8, MAX_PACKET_SIZE>::new();
    write_var_i32(packet_id, &mut body)?;
    body.extend_from_slice(payload)
        .map_err(|_| Error::BufferTooSmall)?;

    let mut packet = Vec::<u8, MAX_PACKET_SIZE>::new();
    write_var_i32(body.len() as i32, &mut packet)?;
    packet
        .extend_from_slice(&body)
        .map_err(|_| Error::BufferTooSmall)?;
    Ok(packet)
}

pub fn string_payload(value: &str) -> Result<Vec<u8, MAX_PACKET_SIZE>, Error> {
    let mut payload = Vec::<u8, MAX_PACKET_SIZE>::new();
    write_var_i32(value.len() as i32, &mut payload)?;
    payload
        .extend_from_slice(value.as_bytes())
        .map_err(|_| Error::BufferTooSmall)?;
    Ok(payload)
}
