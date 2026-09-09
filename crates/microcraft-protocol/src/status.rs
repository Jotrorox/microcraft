use core::fmt::Write;
use heapless::String;

use crate::protocol::Error;

pub const SERVER_PORT: u16 = 25565;
pub const MAX_PLAYERS: u32 = 1;
pub const ONLINE_PLAYERS: u32 = 0;
pub const MOTD: &str = "Microcraft XIAO ESP32-S3";
pub const JSON_CAPACITY: usize = 384;

/// Build-time overrides are checked during compilation, before flashing.
pub const MINECRAFT_VERSION: &str = match option_env!("MINECRAFT_VERSION") {
    Some(version) => {
        assert!(
            valid_version(version),
            "MINECRAFT_VERSION must be 1..64 UTF-8 bytes and at most 128 JSON-escaped bytes"
        );
        version
    }
    None => "26.1.2",
};
pub const PROTOCOL_VERSION: i32 = match option_env!("MINECRAFT_PROTOCOL_VERSION") {
    Some(value) => match parse_protocol(value) {
        Some(protocol) => protocol,
        None => panic!("MINECRAFT_PROTOCOL_VERSION must be a non-negative i32 integer"),
    },
    None => 775,
};

pub const fn parse_protocol(value: &str) -> Option<i32> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut out = 0i32;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte < b'0' || byte > b'9' {
            return None;
        }
        out = match out.checked_mul(10) {
            Some(value) => value,
            None => return None,
        };
        out = match out.checked_add((byte - b'0') as i32) {
            Some(value) => value,
            None => return None,
        };
        index += 1;
    }
    Some(out)
}

pub const fn valid_version(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let mut escaped = 0;
    let mut i = 0;
    while i < bytes.len() {
        escaped += match bytes[i] {
            0..=31 => 6,
            b'"' | b'\\' => 2,
            _ => 1,
        };
        i += 1;
    }
    escaped <= 128
}

/// Clear partial output on failure, so callers cannot send truncated JSON.
pub fn status_json<const N: usize>(
    version: &str,
    protocol: i32,
    out: &mut String<N>,
) -> Result<(), Error> {
    out.clear();
    if !valid_version(version) || protocol < 0 {
        return Err(Error::InvalidPacket);
    }
    if write_json(version, protocol, out).is_err() {
        out.clear();
        return Err(Error::BufferTooSmall);
    }
    Ok(())
}

fn write_json(version: &str, protocol: i32, out: &mut impl Write) -> core::fmt::Result {
    out.write_str("{\"version\":{\"name\":")?;
    json_string(version, out)?;
    write!(
        out,
        ",\"protocol\":{protocol}}},\"players\":{{\"max\":{MAX_PLAYERS},\"online\":{ONLINE_PLAYERS}}},\"description\":{{\"text\":"
    )?;
    json_string(MOTD, out)?;
    out.write_str("}}")
}

fn json_string(value: &str, out: &mut impl Write) -> core::fmt::Result {
    out.write_char('"')?;
    for ch in value.chars() {
        match ch {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            '\u{0}'..='\u{1f}' => write!(out, "\\u{:04x}", ch as u32)?,
            _ => out.write_char(ch)?,
        }
    }
    out.write_char('"')
}
