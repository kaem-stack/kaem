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
cargo test -p kaem-modem            # one crate's tests
cargo test -p kaem-radio-pipeline message_survives_the_modulated_link    # a single test
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
  - `radio` — full signal path: the orchestrator's `RadioPipeline` runs
    fragment -> FSK-modulate -> IQ over `UdpChannel`, and the reverse
    (demodulate -> CRC -> reassemble) on receive.
  - `udp` — raw byte frames in datagrams; skips the modem (fast, for layers above the modem).
  - `loopback` — in-process echo; no peer, no socket (run solo).
- `KAEM_NODE` = `a` | `b` — presets bind/peer ports and callsign. **Does not pick the transport.**
- `KAEM_BIND` / `KAEM_PEER` / `KAEM_CALLSIGN` override the presets (e.g. broadcast across a LAN).

## Architecture

A Cargo virtual workspace of eight **independent, self-contained library
crates** under `crates/` and three crates under `binaries/`. Each library
crate has **zero dependencies on any other `kaem` crate** — they are parts of
one protocol, decoupled on purpose so any of them can later become its own
binary/process. Each crate is a **pure capability that does exactly one job
and never calls another crate**; the binary is the **orchestrator** that calls
them in sequence (on receive: `channel -> modem -> fragment -> mesh/crypto ->
node`; on send, the reverse). There is no composition/"glue" type inside any
library crate — composition lives only in a binary, including injecting one
crate's behavior into another via a trait the *consuming* crate owns (see
`CryptoOps` and `Channel` below), never by one crate naming another in its
`Cargo.toml`.

```
crates/  (one job each, zero kaem deps)    binaries/  (orchestrators)
  kaem-modem    bytes <-> IQ (the codec)     kaem                 (TUI chat)
  kaem-fragment frame <-> MTU fragments       kaem-sandbox         (egui sim)
  kaem-channel  IQ over a medium              kaem-radio-pipeline  (shared radio
  kaem-link     the Transport port                                  composition lib)
  kaem-sim      the RF simulation
  kaem-node     the chat core
  kaem-mesh     the encrypted mesh
  kaem-crypto   crypto primitives
```

(`kaem-radio-pipeline` lives under `binaries/` but is a library, not a runnable
binary: it's binary-side composition code shared by the two real binaries, so
it's allowed to depend on several capability crates — the invariant only
governs `crates/*`.)

This is the non-negotiable invariant: **no `crates/*` crate may depend on
another `crates/*` crate, under any profile, including `dev-dependencies`.**
Shared contracts live with whoever owns them (each crate carries its own
framing + CRC, or its own small trait for a capability it needs but doesn't
own the implementation of), and anything that needs to cross a crate boundary
does so as bytes or a binary-supplied trait object, wired by a binary.

### `kaem-link` — the Transport port

The contract every link speaks, plus the non-radio dev links:

- `Transport { send, recv }` + `TransportError` — the port every link speaks;
  `recv` is non-blocking (`Ok(None)` when nothing is ready). The trait lives
  here and is re-exported; a binary picks/builds which impl to use.
- `UdpTransport` / `Loopback` — dev scaffolding that skips the modem entirely
  (raw datagrams, and in-process echo for running solo), behind the
  `dev-transports` cargo feature (off by default; the `kaem` binary enables it,
  `kaem-sandbox` does not). They are full `Transport` impls, not part of the
  radio signal chain.

`kaem-link` owns no DSP and no radio composition — the modem, fragmentation,
and channel each live in their own crate (below), and the radio signal chain is
assembled by an orchestrator (`RadioPipeline`, see composition roots).

### `kaem-modem` — the FSK codec (bytes <-> IQ)

A binary-FSK software modem, pure DSP with no I/O. `modulate` turns a byte
frame into a framed bitstream (preamble, sync word, len, payload, crc16) then
complex baseband `Iq` samples; `demodulate` recovers the bytes via a quadrature
frequency discriminator. A frame that fails CRC is dropped like line noise — so
a corrupted fragment never reaches the reassembler. Carries its own `Iq` type.

### `kaem-fragment` — message fragmentation

`Fragmenter` splits a whole frame into ordered, `msg_id`/`total`/`index`-tagged
pieces sized to the over-the-air MTU; `Reassembler` rebuilds it, tolerating
out-of-order and duplicate delivery. Sits above the modem, so the chat/mesh
layers keep handing whole frames down and never learn the air has a size limit.
Best-effort to match the link: a dropped fragment leaves its message
incomplete, and the reassembler bounds half-assembled messages by count (no
clock needed). Pure logic, no I/O.

### `kaem-channel` — IQ over a medium

`trait Channel { transmit, receive, local_addr }` + `ChannelError`, the seam
between DSP and the radio. `UdpChannel` carries IQ bursts over UDP (the
simulated airwaves), fragmenting/reassembling bursts larger than one datagram
(a separate, IQ-level concern from the message fragmentation above). A real SDR
(SoapySDR/HackRF/Pluto) is just another `Channel`. Carries its own `Iq` type,
independent of `kaem-modem`'s — the orchestrator converts between them at the
boundary (the same way `kaem-sim` keeps its own `Iq`).

**To add a radio front-end:** add a `Channel` impl (in `kaem-channel` or its own
crate) and wire it into the `RadioPipeline` the orchestrator builds — no
`crates/*` crate changes to compose it. **To add a non-radio link:** add a
`Transport` impl in `kaem-link` and one match arm in the binary's
`Settings::open()`.

### `kaem-sim` — the in-process RF simulation

`Medium` + `NodeId` + `Pos` — an in-process RF field carrying its own `Iq`
samples between in-memory nodes positioned in 2D, each delivery subject to a
seeded Bernoulli loss. Single-threaded by design (`Rc<RefCell<_>>`), so the
sandbox's `step` mode is exactly reproducible. It knows nothing of
`kaem-channel`'s `Channel` trait or `Iq` type — `kaem-sandbox`'s
`SimChannelAdapter` is what implements `kaem-channel`'s `Channel` over a
`Medium`, converting between the two crates' independent `Iq` types at the
boundary.

### `kaem-node` — the chat core

The steppable chat domain, with no transport, clock, or crypto. `Node` owns a
callsign and contact list, takes a virtual `Time` on every call, and hands back
the bytes it wants transmitted (`Outbound`) — the caller owns the link. Carries
its own chat wire protocol in `wire`: `WireMessage { from, to, body }` <-> a
self-describing byte frame (magic `KM`, length-prefixed fields, CRC-16/CCITT).
The same core drives the live TUI and the sandbox.

### `kaem-crypto` — crypto primitives

Pure keygen / hybrid KEM+AEAD encrypt-decrypt / direct symmetric seal-open
functions (ML-KEM-768 + ChaCha20-Poly1305, behind a `Scheme` trait +
`factory::create` dispatch so another algorithm can slot in later). No
chatroom, identity, or relay concepts — it only turns keys and bytes into
other bytes. Nothing in this crate's surface mentions `kaem-mesh`.

### `kaem-mesh` — the encrypted flood-relay mesh

Chatroom pairing plus a **bytes-in/bytes-out** relay — it knows nothing about
chat, and never names `kaem-crypto` (not even in `[dev-dependencies]` — its
own tests duplicate a minimal crypto backend rather than import one, the same
way wire framing is duplicated rather than shared). Every crypto operation it
needs goes through `CryptoOps`, a trait it defines and takes as
`Box<dyn CryptoOps>` at construction — the same trait-injection shape used for
`Channel`. The chatroom `Store` (sqlite) is likewise injected via a
`ChatroomStore` trait the crate defines (the concrete sqlite `Store` lives in
`pairing::store`, in-memory for now). It also owns the identity + pairing
handshake and the `Envelope { chatroom_id, message_id, ttl, ciphertext }` frame
(magic `KE`, distinct from `KM` so the two can never be confused while
decoding).

`MeshNode` exposes:
- `new(crypto: Box<dyn CryptoOps>, store: Box<dyn ChatroomStore>)` — a binary
  supplies the crypto backend (typically `kaem-crypto` wrapped in an adapter)
  and the store; this crate never depends on either implementation directly.
- `begin_pairing` / `finish_pairing` — mint/recover a shared chatroom key via a
  real KEM encapsulation (through `crypto`).
- `seal(to, payload) -> Option<Vec<u8>>` — seal opaque bytes for a paired peer
  into an envelope frame.
- `on_frame(frame) -> Inbound { payload, relay }` — decrypt the payload if it's
  addressed to a chatroom we hold, and report a decremented-TTL relay regardless
  (so a node that can't read an envelope still carries it onward).

### Composition roots (the binaries)

- `binaries/kaem-radio-pipeline` — the radio signal chain as an orchestrator,
  not a library capability. `RadioPipeline` implements `Transport` by composing
  `kaem-fragment` + `kaem-modem` + a `Box<dyn kaem_channel::Channel>`: `send`
  runs fragment -> modulate -> `channel.transmit`; `recv` runs
  `channel.receive` -> demodulate (drop on CRC fail) -> reassemble. It is the
  `kaem_modem::Iq` <-> `kaem_channel::Iq` conversion boundary. MTU is a
  construction parameter (`with_mtu`). Shared by both real binaries so the glue
  lives once; depends on several capability crates because it is composition
  code, not a protocol crate.
- `binaries/kaem` — the CLI/TUI chat binary; wires `kaem-link` + `kaem-node` +
  `kaem-radio-pipeline`. `Settings::open()` in `main.rs` is the factory that
  maps `KAEM_TRANSPORT` to a concrete `Box<dyn Transport>` (`radio` builds a
  `RadioPipeline` over a `UdpChannel`; `udp`/`loopback` are the dev transports).
  The chat domain (`core/chat.rs`) wraps `Node` plus UI-only state (open
  contact, input buffer); the UI is ratatui (`tui/`), one `Widget` per panel.
  UI tick: `chat.poll()` drains frames, draw, then one input action.
- `binaries/kaem-sandbox` — the Packet Tracer-style operator console (egui);
  wires every crate. Each node owns a `Node` (chat) alongside a `MeshNode`
  (crypto), both over a `RadioPipeline`. The binary builds `KaemCrypto`
  (`CryptoOps` over `kaem-crypto`), an in-memory `ChatroomStore`, and
  `SimChannelAdapter` (`kaem-channel`'s `Channel` over a shared
  `kaem-sim::Medium`) and hands them in — no library crate knows the others
  exist. Sending runs `node.command -> mesh.seal -> link`; receiving runs
  `mesh.on_frame -> node.on_frame` (the chat layer drops own echoes). See
  `docs/SANDBOX_PLAN.md`.

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
were then consolidated into three self-contained, mutually-independent crates
(`kaem-link`, `kaem-node`, `kaem-mesh`), then decoupled into five by pulling
`kaem-sim` out of `kaem-link` and `kaem-crypto` out of `kaem-mesh`, and then
`kaem-link` itself was split into the pure capability crates `kaem-modem` /
`kaem-fragment` / `kaem-channel` (with `RadioTransport` dissolved into the
orchestrator's `RadioPipeline`). The end goal: every capability crate becomes
its own binary/process in a Linux distro, with the orchestrator calling them in
a pipeline (over IPC once they are separate processes) — so no crate may ever
name another. `kaem-mesh` is the next god-crate to decompose the same way
(relay vs. pairing/identity/store). Treat `docs/SANDBOX_PLAN.md` as the
historical map of the sandbox work; the `Architecture` section above is the
source of truth for the current layout and is kept in sync with whatever has
actually landed.
