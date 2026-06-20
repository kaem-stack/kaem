mod action;
mod app;
mod core;
mod tui;

use std::net::SocketAddr;

use color_eyre::Result;
use kaem_link::Transport;

use crate::app::App;

fn main() -> Result<()> {
    color_eyre::install()?;
    let settings = Settings::from_env();

    // Composition root: the one place that knows the concrete transport crates
    // and selects one. The transport port and the chat domain never see them.
    let transport = settings.open()?;

    let terminal = ratatui::init();
    let result = App::new(transport, settings.callsign).run(terminal);
    ratatui::restore();
    result
}

/// Which transport adapter to build.
enum Backend {
    Loopback,
    Udp,
    Radio,
}

struct Settings {
    backend: Backend,
    bind: SocketAddr,
    peer: SocketAddr,
    callsign: String,
}

impl Settings {
    /// Resolve identity and transport from the environment so two instances can
    /// be launched side by side without rebuilding:
    ///
    /// * `KAEM_TRANSPORT` `loopback` | `udp` | `radio`  (default `radio`)
    /// * `KAEM_NODE`      `a` | `b` — presets the bind/peer ports and callsign
    /// * `KAEM_CALLSIGN`  overrides the callsign
    /// * `KAEM_BIND` / `KAEM_PEER` override the socket addresses
    ///
    /// The defaults make node `a` (alice, 7001→7002) and node `b` (bob,
    /// 7002→7001) talk to each other on localhost.
    fn from_env() -> Self {
        let node = std::env::var("KAEM_NODE").unwrap_or_else(|_| "a".into());
        let (default_bind, default_peer, default_callsign) = match node.as_str() {
            "b" => ("127.0.0.1:7002", "127.0.0.1:7001", "bob"),
            _ => ("127.0.0.1:7001", "127.0.0.1:7002", "alice"),
        };

        let backend = match std::env::var("KAEM_TRANSPORT").as_deref() {
            Ok("loopback") => Backend::Loopback,
            Ok("udp") => Backend::Udp,
            _ => Backend::Radio,
        };

        Settings {
            backend,
            bind: addr_from_env("KAEM_BIND", default_bind),
            peer: addr_from_env("KAEM_PEER", default_peer),
            callsign: std::env::var("KAEM_CALLSIGN").unwrap_or_else(|_| default_callsign.into()),
        }
    }

    /// Build the selected transport. Adding a protocol is one arm here plus a
    /// dependency in `Cargo.toml` — nothing else in the workspace changes.
    fn open(&self) -> Result<Box<dyn Transport>> {
        Ok(match self.backend {
            Backend::Loopback => Box::new(kaem_link::Loopback::new()),
            Backend::Udp => Box::new(kaem_link::UdpTransport::bind(self.bind, self.peer)?),
            Backend::Radio => Box::new(kaem_radio_pipeline::RadioPipeline::bind(
                self.bind, self.peer,
            )?),
        })
    }
}

fn addr_from_env(var: &str, default: &str) -> SocketAddr {
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| default.parse().expect("valid default address"))
}
