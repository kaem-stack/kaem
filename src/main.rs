mod action;
mod app;
mod codec;
mod core;
mod radio;
mod tui;

use std::net::SocketAddr;

use color_eyre::Result;

use crate::app::App;
use crate::radio::{Config, Link};

fn main() -> Result<()> {
    color_eyre::install()?;
    let (config, callsign) = settings_from_env();

    // All connection logic lives in the radio module; here we only choose what
    // to open and hand the live transport to the app.
    let radio = radio::open(config)?;

    let terminal = ratatui::init();
    let result = App::new(radio, callsign).run(terminal);
    ratatui::restore();
    result
}

/// Resolve the node's identity and transport from the environment so two
/// instances can be launched side by side without rebuilding:
///
/// * `KAEM_RADIO`    `loopback` | `udp` | `sdr`  (default `sdr`)
/// * `KAEM_NODE`     `a` | `b` — presets the bind/peer ports and callsign
/// * `KAEM_CALLSIGN` overrides the callsign
/// * `KAEM_BIND` / `KAEM_PEER` override the socket addresses
///
/// The defaults make node `a` (alice, 7001→7002) and node `b` (bob, 7002→7001)
/// talk to each other on localhost.
fn settings_from_env() -> (Config, String) {
    let node = std::env::var("KAEM_NODE").unwrap_or_else(|_| "a".into());
    let (default_bind, default_peer, default_callsign) = match node.as_str() {
        "b" => ("127.0.0.1:7002", "127.0.0.1:7001", "bob"),
        _ => ("127.0.0.1:7001", "127.0.0.1:7002", "alice"),
    };

    let callsign = std::env::var("KAEM_CALLSIGN").unwrap_or_else(|_| default_callsign.into());
    let link = Link {
        bind: addr_from_env("KAEM_BIND", default_bind),
        peer: addr_from_env("KAEM_PEER", default_peer),
    };

    let config = match std::env::var("KAEM_RADIO").as_deref() {
        Ok("loopback") => Config::Loopback,
        Ok("udp") => Config::Udp(link),
        _ => Config::Sdr(link),
    };

    (config, callsign)
}

fn addr_from_env(var: &str, default: &str) -> SocketAddr {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| default.parse().expect("valid default address"))
}
