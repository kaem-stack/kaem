//! End-to-end across the crate boundary: the same path `main` wires up —
//! the `kaem-radio` factory plus the `kaem-codec` wire protocol — carrying a
//! message between two SDR nodes intact.

use std::time::Duration;

use kaem_codec::{WireMessage, decode, encode};
use kaem_radio::{Config, Link, open};

#[test]
fn message_flows_through_the_sdr_factory() {
    let a = Link {
        bind: "127.0.0.1:17071".parse().unwrap(),
        peer: "127.0.0.1:17072".parse().unwrap(),
    };
    let b = Link {
        bind: "127.0.0.1:17072".parse().unwrap(),
        peer: "127.0.0.1:17071".parse().unwrap(),
    };
    let mut alice = open(Config::Sdr(a)).unwrap();
    let mut bob = open(Config::Sdr(b)).unwrap();

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
