use core::{
    future::Future,
    pin::pin,
    task::{Context, Poll, Waker},
};
use embedded_io_async::{ErrorKind, ErrorType, Read, Write};
use microcraft_protocol::{
    protocol::{self, Cursor, Error, Packet},
    session, status,
};

fn run<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..10000 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
    panic!("mock IO did not complete");
}

#[derive(Default)]
struct MockIo {
    input: Vec<u8>,
    output: Vec<u8>,
    pos: usize,
    chunk: usize,
    read_sizes: Vec<usize>,
    error: Option<ErrorKind>,
    zero_write: bool,
}
impl MockIo {
    fn new(input: Vec<u8>, chunk: usize) -> Self {
        Self {
            input,
            chunk,
            ..Self::default()
        }
    }
}
impl ErrorType for MockIo {
    type Error = ErrorKind;
}
async fn yield_once() {
    let mut yielded = false;
    core::future::poll_fn(|cx| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}
impl Read for MockIo {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, ErrorKind> {
        yield_once().await;
        if let Some(error) = self.error {
            return Err(error);
        }
        self.read_sizes.push(buf.len());
        let len = buf.len().min(self.chunk).min(self.input.len() - self.pos);
        buf[..len].copy_from_slice(&self.input[self.pos..self.pos + len]);
        self.pos += len;
        Ok(len)
    }
}
impl Write for MockIo {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, ErrorKind> {
        yield_once().await;
        if self.zero_write {
            return Ok(0);
        }
        let len = buf.len().min(self.chunk);
        self.output.extend_from_slice(&buf[..len]);
        Ok(len)
    }
    async fn flush(&mut self) -> Result<(), ErrorKind> {
        yield_once().await;
        Ok(())
    }
}
fn handshake() -> Vec<u8> {
    b"\x10\x00\x87\x06\x09localhost\x63\xdd\x01".to_vec()
}
fn exchange(ping: &[u8]) -> Vec<u8> {
    let mut input = handshake();
    input.extend_from_slice(&[1, 0]);
    input.extend_from_slice(ping);
    input
}

#[test]
fn varints_roundtrip_signed_boundaries() {
    for value in [0, 1, 127, 128, 255, 16383, 16384, i32::MAX, -1, i32::MIN] {
        let mut out = Packet::new();
        protocol::write_var_i32(value, &mut out).unwrap();
        assert_eq!(Cursor::new(&out).read_var_i32(), Ok(value));
        assert_eq!(out.len(), protocol::var_i32_len(value));
    }
}
#[test]
fn malformed_varints_are_rejected() {
    for bytes in [
        &[0x80, 0x80, 0x80, 0x80, 0x10][..],
        &[0x80, 0x80, 0x80, 0x80, 0x80, 0],
        &[0xff, 0xff, 0xff, 0xff, 0x7f],
    ] {
        assert_eq!(Cursor::new(bytes).read_var_i32(), Err(Error::InvalidVarInt));
    }
    assert_eq!(
        Cursor::new(&[0x80]).read_var_i32(),
        Err(Error::UnexpectedEof)
    );
}
#[test]
fn strings_validate_utf8_length_and_truncation() {
    assert_eq!(
        Cursor::new(&[1, 255]).read_string(),
        Err(Error::InvalidPacket)
    );
    assert_eq!(
        Cursor::new(&[0x80, 2]).read_string(),
        Err(Error::InvalidPacket)
    );
    assert_eq!(
        Cursor::new(&[2, b'a']).read_string(),
        Err(Error::UnexpectedEof)
    );
    assert_eq!(Cursor::new(&[2, 0xc3, 0xa4]).read_string(), Ok("ä"));
}
#[test]
fn framing_accounts_for_length_prefix_and_clears_old_data() {
    let mut out = Packet::new();
    protocol::make_packet(0, &[0; 509], &mut out).unwrap();
    assert_eq!(out.len(), 512);
    assert_eq!(
        protocol::make_packet(0, &[0; 510], &mut out),
        Err(Error::BufferTooSmall)
    );
    assert!(out.is_empty());
    protocol::make_packet(1, &[0; 8], &mut out).unwrap();
    assert_eq!(&*out, &[9, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
}
#[test]
fn string_packet_capacity_includes_all_prefixes() {
    let mut out = Packet::new();
    protocol::make_string_packet(&"v".repeat(507), &mut out).unwrap();
    assert_eq!(out.len(), 512);
    assert_eq!(
        protocol::make_string_packet(&"v".repeat(508), &mut out),
        Err(Error::BufferTooSmall)
    );
    assert!(out.is_empty());
}
#[test]
fn packet_read_handles_fragmentation_and_coalesced_frames() {
    for chunk in [1, 2, 5, 512] {
        let mut io = MockIo::new(vec![3, 0, 1, 2, 1, 1], chunk);
        let mut packet = Packet::new();
        run(protocol::read_packet(&mut io, &mut packet)).unwrap();
        assert_eq!(&*packet, &[0, 1, 2]);
        run(protocol::read_packet(&mut io, &mut packet)).unwrap();
        assert_eq!(&*packet, &[1]);
    }
}
#[test]
fn full_body_is_requested_in_one_read() {
    let mut io = MockIo::new(vec![3, 0, 1, 2], 512);
    run(protocol::read_packet(&mut io, &mut Packet::new())).unwrap();
    assert_eq!(io.read_sizes, vec![1, 3]);
}
#[test]
fn invalid_network_lengths_are_rejected_before_body_read() {
    for bytes in [
        vec![0],
        vec![0x81, 4],
        vec![0xff, 0xff, 0xff, 0xff, 0x0f],
        vec![0x80, 0x80, 0x80, 0x80, 0x10],
    ] {
        let mut io = MockIo::new(bytes, 512);
        assert!(run(protocol::read_packet(&mut io, &mut Packet::new())).is_err());
    }
}
#[test]
fn eof_distinguishes_clean_close_from_truncated_frames() {
    let mut packet = Packet::new();
    assert_eq!(
        run(protocol::read_packet(
            &mut MockIo::new(vec![], 1),
            &mut packet
        )),
        Err(Error::ConnectionClosed)
    );
    for bytes in [vec![0x80], vec![2, 0]] {
        assert_eq!(
            run(protocol::read_packet(
                &mut MockIo::new(bytes, 1),
                &mut packet
            )),
            Err(Error::UnexpectedEof)
        );
    }
}
#[test]
fn transport_errors_are_preserved() {
    let mut io = MockIo {
        error: Some(ErrorKind::ConnectionReset),
        ..MockIo::default()
    };
    assert_eq!(
        run(protocol::read_packet(&mut io, &mut Packet::new())),
        Err(Error::Io(ErrorKind::ConnectionReset))
    );
}

#[test]
fn a_closed_writer_does_not_panic_or_loop() {
    let mut io = MockIo::new(exchange(&[]), 512);
    io.zero_write = true;
    assert_eq!(
        run(session::handle_connection(
            &mut io,
            &mut Packet::new(),
            &[1, 0]
        )),
        Err(Error::ConnectionClosed)
    );
}
#[test]
fn fragmented_status_and_ping_echo_exact_payload() {
    let ping = [9, 1, 0x80, 0, 1, 2, 3, 4, 5, 0xff];
    for chunk in [1, 2, 512] {
        let mut io = MockIo::new(exchange(&ping), chunk);
        let response = [3, 0, 1, b'x'];
        run(session::handle_connection(
            &mut io,
            &mut Packet::new(),
            &response,
        ))
        .unwrap();
        assert_eq!(io.output, [response.as_slice(), ping.as_slice()].concat());
    }
}
#[test]
fn status_only_clients_can_close_but_partial_ping_is_an_error() {
    let mut io = MockIo::new(exchange(&[]), 1);
    assert_eq!(
        run(session::handle_connection(
            &mut io,
            &mut Packet::new(),
            &[1, 0]
        )),
        Ok(())
    );
    for ping in [&[0x80][..], &[9, 1, 0]] {
        let mut io = MockIo::new(exchange(ping), 1);
        assert_eq!(
            run(session::handle_connection(
                &mut io,
                &mut Packet::new(),
                &[1, 0]
            )),
            Err(Error::UnexpectedEof)
        );
    }
}
#[test]
fn handshake_rejects_login_and_trailing_bytes() {
    let mut packet = handshake()[1..].to_vec();
    *packet.last_mut().unwrap() = 2;
    assert_eq!(
        session::validate_handshake(&packet),
        Err(Error::InvalidPacket)
    );
    *packet.last_mut().unwrap() = 1;
    packet.push(0);
    assert_eq!(
        session::validate_handshake(&packet),
        Err(Error::InvalidPacket)
    );
}
#[test]
fn unexpected_status_and_ping_fields_are_rejected() {
    let mut bytes = handshake();
    bytes.extend_from_slice(&[2, 0, 0]);
    let mut io = MockIo::new(bytes, 512);
    assert_eq!(
        run(session::handle_connection(&mut io, &mut Packet::new(), &[])),
        Err(Error::InvalidPacket)
    );
    assert!(io.output.is_empty());
    for ping in [&[1, 2][..], &[10, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]] {
        let mut io = MockIo::new(exchange(ping), 512);
        assert_eq!(
            run(session::handle_connection(&mut io, &mut Packet::new(), &[])),
            Err(Error::InvalidPacket)
        );
    }
}
#[test]
fn json_escapes_version_and_preserves_unicode() {
    for version in ["26.1.2", "custom\"version\\test\n\t\0", "版本ä"] {
        let mut out = heapless::String::<384>::new();
        status::status_json(version, 775, &mut out).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["version"]["name"], version);
        assert_eq!(parsed["version"]["protocol"], 775);
        assert_eq!(parsed["description"]["text"], status::MOTD);
        let mut packet = Packet::new();
        protocol::make_string_packet(&out, &mut packet).unwrap();
    }
}
#[test]
fn invalid_config_and_json_overflow_do_not_return_partial_json() {
    let mut out = heapless::String::<384>::new();
    for version in ["".to_owned(), "v".repeat(400), "\0".repeat(64)] {
        assert_eq!(
            status::status_json(&version, 775, &mut out),
            Err(Error::InvalidPacket)
        );
        assert!(out.is_empty());
    }
    let mut tiny = heapless::String::<20>::new();
    assert_eq!(
        status::status_json("test", 775, &mut tiny),
        Err(Error::BufferTooSmall)
    );
    assert!(tiny.is_empty());
}
#[test]
fn largest_allowed_version_and_protocol_fit_response() {
    let mut out = heapless::String::<384>::new();
    status::status_json(&"\"".repeat(64), i32::MAX, &mut out).unwrap();
    serde_json::from_str::<serde_json::Value>(&out).unwrap();
    protocol::make_string_packet(&out, &mut Packet::new()).unwrap();
}
#[test]
fn protocol_configuration_requires_nonnegative_i32() {
    assert_eq!(status::parse_protocol("0"), Some(0));
    assert_eq!(status::parse_protocol("775"), Some(775));
    assert_eq!(status::parse_protocol("2147483647"), Some(i32::MAX));
    for value in [
        "",
        "-",
        "-1",
        "+1",
        " 775",
        "775x",
        "2147483648",
        "99999999999999999999",
    ] {
        assert_eq!(status::parse_protocol(value), None, "{value}");
    }
}
