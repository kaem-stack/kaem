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
can be launched side by side without rebuilding (see `binaries/kaem/src/main.rs`):

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
- `binaries/kaem` (the binary) is the **composition root** — the *only* place
  that names concrete adapter crates. `Settings::open()` in `main.rs` is the
  factory that maps `KAEM_TRANSPORT` to an adapter. Nothing else in the
  workspace sees a concrete transport; the chat domain holds a `Box<dyn Transport>`.

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
2. `channel` — `trait Channel { transmit, receive, local_addr }`, the seam
   between DSP and the radio. `UdpChannel` implements it over UDP (the
   simulated airwaves), fragmenting/reassembling IQ bursts that exceed one
   datagram; `kaem-sim`'s `SimChannel` implements it over an in-process medium
   instead. A real SDR (SoapySDR/HackRF/Pluto) becomes a different `Channel`
   impl; the modem and the `Transport` interface above it never change.
   `RadioTransport` holds a `Box<dyn Channel>`, so it's channel-generic.

**Iteration on the radio happens inside `kaem-radio`** (swap the modem or the
channel), not at the `Transport` level.

### Application layers

- `kaem-codec` — the wire protocol. `WireMessage { from, to, body }` <-> a
  self-describing byte frame (magic `KM`). `Envelope { chatroom_id,
  message_id, ttl, ciphertext }` (magic `KE`) is a second, independent frame
  format for the encrypted flood-relay mesh — distinct magic bytes so the two
  can never be confused while decoding. Both use length-prefixed fields and
  CRC-16/CCITT.
- `kaem-crypto` — post-quantum hybrid encryption (ML-KEM-768 KEM +
  ChaCha20-Poly1305), a fresh shared key per message. Split into `keys`
  (generate/persist keypairs), `crypto` (`encrypt`/`decrypt`, each hidden
  behind an algorithm trait + a `factory::create` dispatch so another scheme
  can slot in beside ML-KEM-768 without touching callers), and `symmetric`
  (`seal`/`open` under a chatroom's shared key, for `kaem-mesh`).
- `kaem-node` — the steppable chat core extracted out of the `kaem` binary:
  `Node` owns a callsign and contact list, takes a virtual `Time` on every
  call, and owns no transport/clock itself. Same core drives the live TUI and
  the sandbox.
- `kaem-pairing` — identity + chatroom membership for the mesh: mints an
  ML-KEM-768 keypair (`Identity`) and a `Chatroom { id, peer, key }` via a real
  KEM encapsulation (`handshake::pair`/`accept`); `Store` persists rows
  sqlite-backed (in-memory for now).
- `kaem-mesh` — composes around `kaem-node::Node` to add pairing + encrypted
  flood relay (`MeshNode`), without changing `Node`'s own surface — `kaem`'s
  TUI binary depends on that surface directly and keeps compiling unchanged.
  Seals `WireMessage`s into `Envelope`s under a chatroom key; relays envelopes
  it can't decrypt with a decremented TTL so unpaired hops still carry traffic.
- `kaem-sim` — an in-process RF medium (`Medium`) that carries the same
  `kaem-radio` baseband `Iq` samples a UDP/SDR link would, between in-memory
  nodes positioned in a 2D field; `SimChannel` is the `Channel` impl that
  plugs it into `RadioTransport`. Single-threaded by design (`Rc<RefCell<_>>`),
  so `step` mode in the sandbox is exactly reproducible.
- `binaries/kaem` — the CLI/TUI chat binary. The chat domain (`core/chat.rs`)
  wraps `kaem-node::Node` plus UI-only state (open contact, input buffer); the
  UI is ratatui (`tui/`), one `Widget` per panel. UI tick: `chat.poll()`
  drains received frames, then draw, then handle one input action.
- `binaries/kaem-sandbox` — the Packet Tracer-style operator console (egui):
  spawns nodes into a `kaem-sim::Medium`, places them on a field, drives
  pairing and chat through `kaem-mesh::MeshNode`. See `docs/SANDBOX_PLAN.md`.

## Conventions

- Edition 2024 across the workspace; pinned deps live in the root
  `[workspace.dependencies]` — trust `Cargo.toml` over memory for versions.
- UI is deliberately minimal/brutalist: amber-on-gray, no emojis, no chat
  bubbles. Keep that tone in any TUI work.
- Transmit failures must never take the UI down — the chat domain swallows link
  errors (a status line is the intended place to surface them later).

## Workflow

### Committing

- **Always commit.** Never leave finished work sitting uncommitted in the
  working tree. When a logical slice is done, commit it before moving on.
- **Commit in multiple, small logical commits** — one crate, one seam, one
  refactor per commit — the way the existing history on this repo does it.
  Never bundle unrelated changes into a single commit; split them, even when
  they were written together. `git add -p` a working tree that mixes concerns.
- **Commit message format:** `type(scope): short imperative summary`.
  - `type` is one of: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`,
    `perf`, `style`, `build`.
  - `scope` is usually the crate or module touched (e.g. `radio`, `crypto`,
    `mesh`, `codec`, `sandbox`, `workspace`).
  - `summary` is imperative and lowercase, no trailing period.
  - Examples: `feat(sim): add Medium + SimChannel`, `refactor(radio): rename
    sdr crate to kaem-radio`, `fix(codec): reject envelope with bad crc`.
- Prefer a build/test-green state at each commit where practical — it keeps
  `git bisect` and review useful.

## Planned direction

`docs/SANDBOX_PLAN.md` describes the current effort: a Packet Tracer-style
sandbox that spawns many nodes in one process, simulating the RF medium
in-process instead of over real UDP/SDR. The workspace has already been
reorganized into `crates/` (libraries) and `binaries/` (apps) per that plan;
the doc's "Suggested commit sequence" section is the map of phases:
workspace split -> extract `kaem-node` (steppable chat core) -> make
`kaem-radio` channel-generic -> `kaem-sim` (in-process `Medium`/`SimChannel`)
-> `kaem-pairing`/`kaem-mesh` (ML-KEM pairing + encrypted flood-relay) ->
`binaries/kaem-sandbox` (the egui operator console). Treat that doc as the
source of truth for this work; the layout above (`Architecture`) is kept in
sync with whatever has actually landed.
