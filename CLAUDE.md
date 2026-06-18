# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

kaem is a TUI mesh-radio chat app in Rust (edition 2024). Nodes exchange chat
messages over a radio link that runs entirely in software — a custom FSK modem
whose baseband samples travel over a UDP-simulated channel today, and over real
SDR hardware later. No radio hardware is required to run or develop it.

## Commands

```bash
cargo build                         # build the whole workspace
cargo test                          # all tests, all crates
cargo test -p kaem-radio            # one crate's tests
cargo test -p kaem-radio message_survives_the_modulated_link   # a single test
cargo clippy --all-targets          # lint — keep this clean (edition-2024 let-chains in use)
cargo fmt                           # format
```

### Running the app

The binary chooses identity and transport from the environment, so two instances
can be launched side by side without rebuilding (see `crates/kaem/src/main.rs`):

```bash
# Two nodes that talk to each other over the simulated radio (default transport):
KAEM_NODE=a cargo run -p kaem       # alice, binds 7001 -> 7002
KAEM_NODE=b cargo run -p kaem       # bob,   binds 7002 -> 7001
```

- `KAEM_TRANSPORT` = `radio` (default) | `udp` | `loopback`
  - `radio` — full signal path: FSK-modulate -> IQ over `UdpChannel` -> demodulate -> CRC.
  - `udp` — raw byte frames in datagrams; skips the modem (fast, for layers above the modem).
  - `loopback` — in-process echo; no peer, no socket (run solo).
- `KAEM_NODE` = `a` | `b` — presets bind/peer ports and callsign. **Does not pick the transport.**
- `KAEM_BIND` / `KAEM_PEER` / `KAEM_CALLSIGN` override the presets (e.g. broadcast across a LAN).

## Architecture

A Cargo virtual workspace built as **ports and adapters**. The big picture lives
across several crates; read them as one system:

### Transport port + adapters

- `kaem-transport` — the **port**: `trait Transport { send, recv }` + `TransportError`.
  Depends on nothing and knows none of its implementors. `recv` is non-blocking
  (returns `Ok(None)` when nothing is ready).
- `kaem-radio`, `kaem-udp`, `kaem-loopback` — **adapters**, each its own crate
  depending only on `kaem-transport`. `kaem-radio` is the real target; `kaem-udp`
  and `kaem-loopback` are development scaffolding, not real links.
- `crates/kaem` (the binary) is the **composition root** — the *only* place that
  names concrete adapter crates. `Settings::open()` in `main.rs` is the factory
  that maps `KAEM_TRANSPORT` to an adapter. Nothing else in the workspace sees a
  concrete transport; the chat domain holds a `Box<dyn Transport>`.

**To add a transport:** new crate depending on `kaem-transport`, plus one match
arm in `Settings::open()` and one dependency line. Never add the factory or
adapter selection to `kaem-transport`.

### The radio signal path (`kaem-radio`)

`RadioTransport` is a real RF signal chain in software, layered in two seams:

1. `modem` — a binary-FSK software modem. `modulate` turns a byte frame into a
   framed bitstream (preamble, sync word, len, payload, crc16) and then into
   complex baseband `Iq` samples; `demodulate` recovers the bytes via a
   quadrature frequency discriminator. A frame that fails CRC is dropped like
   line noise.
2. `channel` — `trait Channel { transmit, receive }`, the seam between DSP and
   the radio. `UdpChannel` implements it over UDP (the simulated airwaves),
   fragmenting/reassembling IQ bursts that exceed one datagram. A real SDR
   (SoapySDR/HackRF/Pluto) becomes a different `Channel` impl; the modem and the
   `Transport` interface above it never change.

**Iteration on the radio happens inside `kaem-radio`** (swap the modem or the
channel), not at the `Transport` level.

### Application layers

- `kaem-codec` — the wire protocol: `WireMessage { from, to, body }` <-> a
  self-describing byte frame (magic `KM`, version, length-prefixed fields,
  CRC-16/CCITT). It owns *what* goes over the link, never *how*.
- `kaem-crypto` — post-quantum hybrid encryption (ML-KEM-768 KEM +
  ChaCha20-Poly1305), a fresh shared key per message. Split into `keys`
  (generate/persist keypairs) and `crypto` (`encrypt`/`decrypt`), each hidden
  behind an algorithm trait + a `factory::create` dispatch so another scheme can
  slot in beside ML-KEM-768 without touching callers. **Not yet wired into the
  binary** — it builds as a workspace member but nothing depends on it.
- `crates/kaem` — the CLI/TUI binary. The chat domain (`core/chat.rs`) depends
  only on `Transport` + `kaem-codec`; the UI is ratatui (`tui/`), one `Widget`
  per panel. UI tick: `chat.poll()` drains received frames, then draw, then
  handle one input action.

## Conventions

- Edition 2024 across the workspace; pinned deps live in the root
  `[workspace.dependencies]` — trust `Cargo.toml` over memory for versions.
- UI is deliberately minimal/brutalist: amber-on-gray, no emojis, no chat
  bubbles. Keep that tone in any TUI work.
- Transmit failures must never take the UI down — the chat domain swallows link
  errors (a status line is the intended place to surface them later).

## Planned direction

`docs/SANDBOX_PLAN.md` describes the next major effort: a Packet Tracer-style
sandbox that spawns many nodes in one process, with a workspace reorg into
`crates/` (libraries) and `binaries/` (apps). Treat that doc as the source of
truth for that work; the layout above is the current state.
