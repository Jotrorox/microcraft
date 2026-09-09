pub const SERVER_PORT: u16 = 25565;

/// Version advertised in the Minecraft status response.
///
/// Override at build time with `MINECRAFT_VERSION`, for example:
/// `MINECRAFT_VERSION=26.1.2 MINECRAFT_PROTOCOL_VERSION=775 cargo run ...`
pub const MINECRAFT_VERSION: &str = match option_env!("MINECRAFT_VERSION") {
    Some(version) => version,
    None => "26.1.2",
};

/// Protocol advertised in the Minecraft status response.
///
/// The 26.1.2 client jar reports `RELEASE_NETWORK_PROTOCOL_VERSION = 775`
/// in `net.minecraft.SharedConstants`.
pub const PROTOCOL_VERSION: i32 = match option_env!("MINECRAFT_PROTOCOL_VERSION") {
    Some(protocol) => parse_i32(protocol),
    None => 775,
};

pub const MAX_PLAYERS: u32 = 1;
pub const ONLINE_PLAYERS: u32 = 0;
pub const MOTD: &str = "Microcraft XIAO ESP32-S3";

const fn parse_i32(value: &str) -> i32 {
    let bytes = value.as_bytes();
    let mut index = 0;
    let mut sign = 1;
    let mut out = 0i32;

    if !bytes.is_empty() && bytes[0] == b'-' {
        sign = -1;
        index = 1;
    }

    while index < bytes.len() {
        let byte = bytes[index];
        assert!(
            byte >= b'0' && byte <= b'9',
            "invalid MINECRAFT_PROTOCOL_VERSION"
        );
        out = out * 10 + (byte - b'0') as i32;
        index += 1;
    }

    out * sign
}

pub fn status_json() -> heapless::String<384> {
    let mut json = heapless::String::new();
    let _ = core::fmt::write(
        &mut json,
        format_args!(
            "{{\"version\":{{\"name\":\"{}\",\"protocol\":{}}},\"players\":{{\"max\":{},\"online\":{}}},\"description\":{{\"text\":\"{}\"}}}}",
            MINECRAFT_VERSION, PROTOCOL_VERSION, MAX_PLAYERS, ONLINE_PLAYERS, MOTD
        ),
    );
    json
}
