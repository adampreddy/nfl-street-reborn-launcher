//! Can this PC reach the NetPlay traversal server? Measured, for matchmaking's `ready` preflight.
//!
//! The traversal server is what turns a host into an 8-character code and lets a joiner reach it
//! without port forwarding. If it cannot be reached, the host never gets a code and the match stalls
//! at "Starting the game" until the server's deadline - so the preflight says so up front, as its
//! own answer (`traversal_ok`), separate from "the Reborn backend answered" (`backend_ok`).
//!
//! The probe is exactly the first packet Dolphin itself sends: one `HelloFromClient`, laid out as
//! the fork's `Source/Core/Common/TraversalProto.h` packs it, to the fork's default server
//! (`NetplaySettings.cpp`: stun.dolphin-emu.org, port 6262). A `HelloFromServer` with `ok = 1` back
//! is a yes. Anything else - no DNS, no reply inside the timeout, a refusal - is a no.

use std::net::{ToSocketAddrs, UdpSocket};
use std::time::Duration;

/// The fork's `NETPLAY_TRAVERSAL_SERVER` and `NETPLAY_TRAVERSAL_PORT` defaults.
pub const DEFAULT_SERVER: &str = "stun.dolphin-emu.org:6262";

/// `sizeof(TraversalPacket)` under `#pragma pack(1)`: type (1) + request id (8) + the largest union
/// member, `helloFromServer` = ok (1) + host id (8) + TraversalInetAddress (1 + 16 + 2) = 28.
pub const PACKET_SIZE: usize = 1 + 8 + 28;

const HELLO_FROM_CLIENT: u8 = 2;
const HELLO_FROM_SERVER: u8 = 3;
/// `TraversalProtoVersion`.
const PROTO_VERSION: u8 = 0;

/// A `HelloFromClient`, byte for byte.
pub fn hello_packet(request_id: u64) -> [u8; PACKET_SIZE] {
    let mut p = [0u8; PACKET_SIZE];
    p[0] = HELLO_FROM_CLIENT;
    p[1..9].copy_from_slice(&request_id.to_le_bytes());
    p[9] = PROTO_VERSION;
    p
}

/// `Some(ok)` for a `HelloFromServer`, `None` for anything else.
pub fn hello_reply(buf: &[u8]) -> Option<bool> {
    (buf.len() >= 10 && buf[0] == HELLO_FROM_SERVER).then(|| buf[9] == 1)
}

/// One hello, sent twice at most (UDP drops packets), `timeout` per try. Never panics, never blocks
/// longer than about `3 * timeout`.
pub fn probe(server: &str, timeout: Duration) -> bool {
    // DNS gets its own thread and its own deadline. The standard lookup has no timeout, and on
    // Windows a name that does not resolve took 11 seconds to say so (measured 2026-09-22) - long
    // enough to hold a player's ready past the point the other player gives up.
    let (tx, rx) = std::sync::mpsc::channel();
    let name = server.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(name.to_socket_addrs().ok().and_then(|mut a| a.find(|x| x.is_ipv4())));
    });
    let Some(addr) = rx.recv_timeout(timeout).ok().flatten() else { return false };
    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else { return false };
    if sock.set_read_timeout(Some(timeout)).is_err() {
        return false;
    }
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    let mut buf = [0u8; 512];
    for _ in 0..2 {
        if sock.send_to(&hello_packet(id), addr).is_err() {
            return false;
        }
        while let Ok((n, from)) = sock.recv_from(&mut buf) {
            if from.ip() != addr.ip() {
                continue;
            }
            if let Some(ok) = hello_reply(&buf[..n]) {
                return ok;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hello_is_laid_out_as_the_fork_packs_it() {
        let p = hello_packet(0x0102_0304_0506_0708);
        assert_eq!(p.len(), 37);
        assert_eq!(p[0], 2, "HelloFromClient");
        assert_eq!(&p[1..9], &[8, 7, 6, 5, 4, 3, 2, 1], "request id, little-endian");
        assert_eq!(p[9], 0, "protocol version 0");
        assert!(p[10..].iter().all(|b| *b == 0));
    }

    #[test]
    fn only_an_ok_hello_from_the_server_is_a_yes() {
        let mut r = [0u8; PACKET_SIZE];
        r[0] = 3;
        r[9] = 1;
        assert_eq!(hello_reply(&r), Some(true));
        r[9] = 0;
        assert_eq!(hello_reply(&r), Some(false), "a server refusing our protocol version");
        r[0] = 0;
        assert_eq!(hello_reply(&r), None, "an ack is not a hello");
        assert_eq!(hello_reply(&[3]), None, "a short packet is not a hello");
    }

    #[test]
    fn an_unresolvable_server_is_a_no_and_not_a_hang() {
        let t = std::time::Instant::now();
        assert!(!probe("no-such-host.invalid:6262", Duration::from_millis(200)));
        assert!(t.elapsed() < Duration::from_secs(2), "took {:?}", t.elapsed());
    }
}
