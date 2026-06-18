//! End-to-end across the crate boundary: the same wiring `main` performs —
//! a `kaem-sdr` transport plus the `kaem-codec` wire protocol — carrying a
//! message between two SDR nodes intact.

use std::net::SocketAddr;
use std::time::Duration;

use kaem_codec::{WireMessage, decode, encode};
use kaem_sdr::SdrTransport;
use kaem_transport::Transport;

#[test]
fn message_flows_through_the_sdr_transport() {
    let any: SocketAddr = "127.0.0.1:0".parse().unwrap();

    let mut bob = SdrTransport::bind(any, "127.0.0.1:9".parse().unwrap()).unwrap();
    let bob_addr = bob.local_addr().unwrap();
    let mut alice = SdrTransport::bind(any, bob_addr).unwrap();

    let sent = WireMessage {
        from: "alice".into(),
        to: "bob".into(),
        body: "relay node is up on channel 7".into(),
    };
    alice.send(&encode(&sent)).unwrap();

    let mut received = None;
    for _ in 0..400 {
        if let Some(frame) = bob.recv().unwrap() {
            received = Some(decode(&frame).unwrap());
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(received, Some(sent));
}
