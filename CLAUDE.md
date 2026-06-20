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
cargo test -p kaem-link             # one crate's tests
cargo test -p kaem-link message_survives_the_modulated_link    # a single test
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

A Cargo virtual workspace of three **independent, self-contained library
crates** under `crates/` and two binaries under `binaries/`. Each library crate
has **zero dependencies on any other `kaem` crate** — they are parts of one
protocol, decoupled on purpose so any of them can later become its own
binary/process. Only the binaries depend on the crates, and a binary is the
only place that composes them.

```
crates/                            binaries/
  kaem-link   the link layer         kaem          (TUI chat)  -> link + node
  kaem-node   the chat core          kaem-sandbox  (egui sim)  -> link + node + mesh
  kaem-mesh   the encrypted mesh
```

This is the non-negotiable invariant: **no `crates/*` crate may depend on
another `crates/*` crate.** Shared contracts live with whoever owns them (each
crate carries its own framing + CRC), and anything that needs to cross a crate
boundary does so as bytes, wired by a binary.

### `kaem-link` — the link layer

Everything that moves opaque byte frames between nodes, behind one port:

- `Transport { send, recv }` + `TransportError` — the port every link speaks;
  `recv` is non-blocking (`Ok(None)` when nothing is ready). The trait lives
  here and is re-exported; a binary picks which impl to build.
- `RadioTransport` — the real RF signal chain in software, in two seams:
  1. `modem` — a binary-FSK software modem. `modulate` turns a byte frame into a
     framed bitstream (preamble, sync word, len, payload, crc16) then complex
     baseband `Iq` samples; `demodulate` recovers the bytes via a quadrature
     frequency discriminator. A frame that fails CRC is dropped like line noise.
  2. `channel` — `trait Channel { transmit, receive, local_addr }`, the seam
     between DSP and the radio. `UdpChannel` carries IQ bursts over UDP (the
     simulated airwaves), fragmenting/reassembling bursts larger than one
     datagram; `SimChannel` carries the same samples across the in-process
     `Medium`. A real SDR (SoapySDR/HackRF/Pluto) is just another `Channel`;
     the modem and `Transport` above it never change. `RadioTransport` holds a
     `Box<dyn Channel>`, so it's channel-generic.
- `Medium` + `SimChannel` — an in-process RF field carrying the same `Iq`
  samples between in-memory nodes positioned in 2D, each delivery subject to a
  seeded Bernoulli loss. Single-threaded by design (`Rc<RefCell<_>>`), so the
  sandbox's `step` mode is exactly reproducible.
- `UdpTransport` / `Loopback` — dev scaffolding that skips the modem: raw
  datagrams, and in-process echo for running solo.

**To add a link:** add a `Transport` impl inside `kaem-link` (plus a `Channel`
impl if it's another radio front-end) and one match arm in the binary's
`Settings::open()`. Iteration on the radio (swap the modem or channel) stays
inside `kaem-link`.

### `kaem-node` — the chat core

The steppable chat domain, with no transport, clock, or crypto. `Node` owns a
callsign and contact list, takes a virtual `Time` on every call, and hands back
the bytes it wants transmitted (`Outbound`) — the caller owns the link. Carries
its own chat wire protocol in `wire`: `WireMessage { from, to, body }` <-> a
self-describing byte frame (magic `KM`, length-prefixed fields, CRC-16/CCITT).
The same core drives the live TUI and the sandbox.

### `kaem-mesh` — the encrypted flood-relay mesh

Chatroom pairing plus a **bytes-in/bytes-out** crypto relay — it knows nothing
about chat. Internally it vendors the post-quantum crypto (ML-KEM-768 KEM +
ChaCha20-Poly1305, a fresh key per message, each behind an algorithm trait +
`factory::create` dispatch so another scheme can slot in), the identity +
chatroom `Store` (sqlite, in-memory for now) and pairing handshake, and the
`Envelope { chatroom_id, message_id, ttl, ciphertext }` frame (magic `KE`,
distinct from `KM` so the two can never be confused while decoding).

`MeshNode` exposes:
- `begin_pairing` / `finish_pairing` — mint/recover a shared chatroom key via a
  real KEM encapsulation.
- `seal(to, payload) -> Option<Vec<u8>>` — seal opaque bytes for a paired peer
  into an envelope frame.
- `on_frame(frame) -> Inbound { payload, relay }` — decrypt the payload if it's
  addressed to a chatroom we hold, and report a decremented-TTL relay regardless
  (so a node that can't read an envelope still carries it onward).

### Composition roots (the binaries)

- `binaries/kaem` — the CLI/TUI chat binary; wires `kaem-link` + `kaem-node`.
  `Settings::open()` in `main.rs` is the factory that maps `KAEM_TRANSPORT` to a
  concrete link and hands the chat domain a `Box<dyn Transport>`. The chat
  domain (`core/chat.rs`) wraps `Node` plus UI-only state (open contact, input
  buffer); the UI is ratatui (`tui/`), one `Widget` per panel. UI tick:
  `chat.poll()` drains frames, draw, then one input action.
- `binaries/kaem-sandbox` — the Packet Tracer-style operator console (egui);
  wires all three crates. Each node owns a `Node` (chat) alongside a `MeshNode`
  (crypto), both over a `RadioTransport` on a shared `Medium`. Sending runs
  `node.command -> mesh.seal -> link`; receiving runs `mesh.on_frame ->
  node.on_frame` (the chat layer drops own echoes). See `docs/SANDBOX_PLAN.md`.

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
  - `scope` is usually the crate or module touched (e.g. `link`, `modem`,
    `node`, `mesh`, `sandbox`, `workspace`).
  - `summary` is imperative and lowercase, no trailing period.
  - Examples: `refactor(link): merge transport and radio into kaem-link`,
    `feat(mesh): add encrypted flood relay`, `fix(node): reject frame with bad crc`.
- Prefer a build/test-green state at each commit where practical — it keeps
  `git bisect` and review useful.

## Planned direction

`docs/SANDBOX_PLAN.md` describes the original effort: a Packet Tracer-style
sandbox that spawns many nodes in one process, simulating the RF medium
in-process instead of over real UDP/SDR. That landed, and the library crates
were then consolidated into the three self-contained, mutually-independent
crates above (`kaem-link`, `kaem-node`, `kaem-mesh`) so each can later become
its own binary. Treat that doc as the historical map of the sandbox work; the
`Architecture` section above is the source of truth for the current layout and
is kept in sync with whatever has actually landed.
