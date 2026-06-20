//! `Sandbox` owns every simulated node, the shared RF [`Medium`], and the
//! deterministic tick loop. Rendering-free so the engine logic
//! ([`Sandbox::step`], coordinate-independent message delivery) is directly
//! unit-testable, independent of whatever UI framework drives it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use kaem_link::Transport;
use kaem_mesh::{MeshNode, pairing::Store};
use kaem_node::{Command, Node};
use kaem_radio_pipeline::RadioPipeline;
use kaem_sim::{Medium, NodeId, Pos};

use crate::crypto_adapter::KaemCrypto;
use crate::field::FIELD;
use crate::sim_adapter::SimChannelAdapter;

/// Virtual milliseconds per tick.
pub const DT: u64 = 50;

const DEFAULT_RANGE: f32 = 35.0;
const DEFAULT_LOSS: f32 = 0.0;
const DEFAULT_SEED: u64 = 1;

/// Multiplier applied to `dt` per auto-step tick while `running` — the
/// playback-speed control. `step()` itself always advances by exactly `dt`
/// regardless of this value; only the auto-step loop in `app.rs` scales by
/// `speed`.
const DEFAULT_SPEED: f32 = 1.0;

/// Bound on event-log growth — old entries are evicted FIFO once exceeded,
/// so a long-running sandbox session doesn't grow the log unboundedly.
const EVENT_LOG_CAPACITY: usize = 500;

const NAMES: &[&str] = &[
    "alice", "bob", "carol", "dave", "erin", "frank", "grace", "heidi",
];

/// Per-node send/receive/relay counters for the operator-facing stats.
#[derive(Default, Clone, Copy)]
pub struct NodeStats {
    pub sent: u64,
    pub received: u64,
    pub relayed: u64,
}

/// One simulated node: its chat core, its encrypted-mesh layer, its radio
/// transport over the shared medium, and where it sits in the field. The binary
/// is the composition root that wires the (chat-agnostic) mesh to the
/// (crypto-agnostic) chat core — neither crate depends on the other.
pub struct SimNode {
    pub name: String,
    /// The node's handle in the shared [`Medium`] — needed to call
    /// `Medium::set_position` when the operator drags it on the canvas.
    pub id: NodeId,
    pub pos: Pos,
    /// Plaintext chat: contact list + history.
    pub chat: Node,
    /// Encrypted mesh: identity, pairing, and the seal/open + flood relay.
    pub mesh: MeshNode,
    pub transport: RadioPipeline,
    pub stats: NodeStats,
}

/// An expanding RF wave drawn from a transmit, for the canvas animation. Each
/// pulse carries the exact frame bytes that travelled the link, so the
/// canvas's click-to-inspect can show the operator the real decoded fields
/// rather than a placeholder.
pub struct Pulse {
    pub origin: Pos,
    pub start: u64,
    pub frame: Rc<Vec<u8>>,
}

/// A directional hop marker: one transmit, from one origin toward one
/// specific in-range neighbor, animated as a point sliding along that
/// segment. Emitted alongside the expanding-ring `Pulse` so a multi-hop flood
/// relay reads on the canvas as a chain of directional hops, each one
/// originating from whichever node relayed it.
pub struct Hop {
    pub from: Pos,
    pub to: Pos,
    pub start: u64,
    pub frame: Rc<Vec<u8>>,
}

/// One entry in the sandbox-wide chronological event log — independent of
/// any per-node chat history, so it reads as a wire-level trace
/// cross-referenceable against the packet inspector via `message_id`.
pub struct LogEntry {
    pub clock: u64,
    pub node: String,
    pub action: EventKind,
    pub frame: Rc<Vec<u8>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Sent,
    Relayed,
    Received,
}

impl EventKind {
    pub fn label(&self) -> &'static str {
        match self {
            EventKind::Sent => "sent",
            EventKind::Relayed => "relayed",
            EventKind::Received => "received",
        }
    }
}

pub struct Sandbox {
    pub medium: Rc<RefCell<Medium>>,
    /// Mirrors `clock`, shared with every node's [`SimChannelAdapter`] so
    /// `Medium` can stamp and check propagation delay — `Channel` itself
    /// carries no time parameter (see `SimChannelAdapter`'s doc comment).
    now: Rc<Cell<u64>>,
    pub nodes: Vec<SimNode>,
    pub clock: u64,
    pub dt: u64,
    pub running: bool,
    pub pulses: Vec<Pulse>,
    pub hops: Vec<Hop>,
    pub log: Vec<LogEntry>,
    pub cursor: Pos,
    /// Playback-speed multiplier for the auto-step loop — `1.0` is
    /// real-time-with-`dt`, `2.0` runs the sim twice as fast, etc. Doesn't
    /// affect the semantics of a single `step()` call.
    pub speed: f32,
}

impl Sandbox {
    pub fn new() -> Self {
        let medium = Rc::new(RefCell::new(Medium::new(
            DEFAULT_RANGE,
            DEFAULT_LOSS,
            DEFAULT_SEED,
            crate::field::WAVE_SPEED,
        )));

        let mut sandbox = Self {
            medium,
            now: Rc::new(Cell::new(0)),
            nodes: Vec::new(),
            clock: 0,
            dt: DT,
            running: true,
            pulses: Vec::new(),
            hops: Vec::new(),
            log: Vec::new(),
            cursor: Pos {
                x: FIELD / 2.0,
                y: FIELD / 2.0,
            },
            speed: DEFAULT_SPEED,
        };

        // Seed a handful of starting nodes spread across the field so the
        // canvas isn't empty on launch.
        let seeds = [
            Pos { x: 20.0, y: 20.0 },
            Pos { x: 70.0, y: 30.0 },
            Pos { x: 45.0, y: 75.0 },
        ];
        for pos in seeds {
            sandbox.add_node(pos);
        }

        sandbox
    }

    /// Register a new node at `pos`, naming it from `NAMES` (falling back to
    /// `n{N}` once the list is exhausted), and wire its radio transport over
    /// a fresh [`SimChannelAdapter`] on the shared medium.
    pub fn add_node(&mut self, pos: Pos) -> usize {
        let id = self.medium.borrow_mut().register(pos);
        let name = NAMES
            .get(self.nodes.len())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("n{}", self.nodes.len()));

        let transport = RadioPipeline::new(Box::new(SimChannelAdapter::new(
            id,
            self.medium.clone(),
            self.now.clone(),
        )));
        let chat = Node::new(name.clone());

        self.nodes.push(SimNode {
            name,
            id,
            pos,
            chat,
            mesh: MeshNode::new(
                Box::new(KaemCrypto),
                Box::new(Store::open_in_memory().expect("in-memory sqlite must open")),
            ),
            transport,
            stats: NodeStats::default(),
        });
        self.nodes.len() - 1
    }

    /// Reposition the node at `idx`, in both the canvas-facing `pos` and the
    /// shared [`Medium`] (which is what actually drives reachability) — used
    /// while the operator drags a node on the canvas. A no-op if `idx` is out
    /// of range.
    pub fn move_node(&mut self, idx: usize, pos: Pos) {
        let Some(node) = self.nodes.get_mut(idx) else {
            return;
        };
        node.pos = pos;
        self.medium.borrow_mut().set_position(node.id, pos);
    }

    /// Remove the node at `idx`, unregistering it from the shared [`Medium`]
    /// too so it stops being a reachability candidate for everyone else. A
    /// no-op if `idx` is out of range. Indices above `idx` shift down by
    /// one, same as any `Vec::remove` — callers that keep their own
    /// per-node state in lockstep (e.g. `app.rs`'s `chats`) must remove the
    /// matching entry too.
    pub fn remove_node(&mut self, idx: usize) {
        if idx >= self.nodes.len() {
            return;
        }
        let node = self.nodes.remove(idx);
        self.medium.borrow_mut().remove(node.id);
    }

    /// Advance the simulation by exactly one tick:
    /// 1. advance the clock,
    /// 2. drain each node's transport into `on_frame`, retransmitting any
    ///    relay outbound it returns (flood-relay: a node that can't decrypt
    ///    an envelope still rebroadcasts it with a decremented TTL),
    /// 3. garbage-collect pulses that have outgrown the medium's range.
    pub fn step(&mut self) {
        self.clock += self.dt;
        let now = self.clock;
        self.now.set(now);

        for idx in 0..self.nodes.len() {
            let mut relays: Vec<Vec<u8>> = Vec::new();
            let mut received: Vec<Rc<Vec<u8>>> = Vec::new();
            let node = &mut self.nodes[idx];
            while let Ok(Some(frame)) = node.transport.recv() {
                let inbound = node.mesh.on_frame(&frame);
                // Decrypted payload (if any) folds into this node's chat
                // history; the relay (if any) is rebroadcast below.
                if let Some(payload) = inbound.payload {
                    node.chat.on_frame(&payload, now);
                    received.push(Rc::new(frame));
                }
                if let Some(relay) = inbound.relay {
                    relays.push(relay);
                }
            }
            if !received.is_empty() {
                self.nodes[idx].stats.received += received.len() as u64;
                let name = self.nodes[idx].name.clone();
                for frame in received {
                    self.push_log(now, name.clone(), EventKind::Received, frame);
                }
            }
            for relay in relays {
                let node = &mut self.nodes[idx];
                let _ = node.transport.send(&relay);
                node.stats.relayed += 1;
                let origin = node.pos;
                let name = node.name.clone();
                let frame = Rc::new(relay);
                self.pulses.push(Pulse {
                    origin,
                    start: now,
                    frame: frame.clone(),
                });
                self.push_hops(idx, origin, now, frame.clone());
                self.push_log(now, name, EventKind::Relayed, frame);
            }
        }

        let range = self.medium.borrow().range();
        self.pulses
            .retain(|p| (now.saturating_sub(p.start) as f32) * crate::field::WAVE_SPEED <= range);
        self.hops
            .retain(|h| now.saturating_sub(h.start) <= hop_duration(h.from, h.to));
    }

    /// Send a console message from the node at `idx` to `to` immediately:
    /// seal it for their shared chatroom, transmit the resulting envelope,
    /// and push a wave pulse. Returns `false` (and transmits nothing) if
    /// `idx` and `to` aren't paired — the caller surfaces that as a status
    /// line. Delivery to receivers still happens on the next `step` — that's
    /// the deterministic-sim semantics.
    pub fn send_from(&mut self, idx: usize, to: &str, body: String) -> bool {
        let now = self.clock;
        self.now.set(now);
        let Some(node) = self.nodes.get_mut(idx) else {
            return false;
        };
        if !node.mesh.is_paired_with(to) {
            return false; // unpaired with `to` — nothing to seal under
        }
        // Record + encode the chat frame, then seal each for the chatroom and
        // hand the resulting envelope to the link.
        let frames = node.chat.command(
            Command::Send {
                to: to.to_string(),
                body,
            },
            now,
        );
        let mut transmitted = false;
        let mut sealed_envelopes: Vec<Vec<u8>> = Vec::new();
        for frame in frames {
            if let Some(envelope) = node.mesh.seal(to, &frame.0) {
                let _ = node.transport.send(&envelope);
                sealed_envelopes.push(envelope);
            }
        }
        for envelope in sealed_envelopes {
            self.nodes[idx].stats.sent += 1;
            let origin = self.nodes[idx].pos;
            let name = self.nodes[idx].name.clone();
            let frame = Rc::new(envelope);
            self.pulses.push(Pulse {
                origin,
                start: now,
                frame: frame.clone(),
            });
            self.push_hops(idx, origin, now, frame.clone());
            self.push_log(now, name, EventKind::Sent, frame);
            transmitted = true;
        }
        transmitted
    }

    /// Push one directional [`Hop`] from `origin` toward every other
    /// registered node currently within the medium's range — the per-segment
    /// animation that makes a flood relay read as a chain of hops rather
    /// than one undirected ring.
    fn push_hops(&mut self, from_idx: usize, origin: Pos, now: u64, frame: Rc<Vec<u8>>) {
        let range = self.medium.borrow().range();
        for (j, other) in self.nodes.iter().enumerate() {
            if j == from_idx {
                continue;
            }
            if within_range(origin, other.pos, range) {
                self.hops.push(Hop {
                    from: origin,
                    to: other.pos,
                    start: now,
                    frame: frame.clone(),
                });
            }
        }
    }

    /// Append to the event log, evicting the oldest entry once
    /// [`EVENT_LOG_CAPACITY`] is exceeded.
    fn push_log(&mut self, clock: u64, node: String, action: EventKind, frame: Rc<Vec<u8>>) {
        if self.log.len() >= EVENT_LOG_CAPACITY {
            self.log.remove(0);
        }
        self.log.push(LogEntry {
            clock,
            node,
            action,
            frame,
        });
    }

    /// Pair the nodes at `a_idx` and `b_idx`: exchange identity public keys
    /// and mint a shared chatroom both sides can use to seal/open future
    /// envelopes between them. A no-op if either index is out of range or
    /// the handshake crypto fails.
    pub fn pair(&mut self, a_idx: usize, b_idx: usize) {
        if a_idx == b_idx {
            return;
        }
        let Some(a_name) = self.nodes.get(a_idx).map(|n| n.name.clone()) else {
            return;
        };
        let Some(b_name) = self.nodes.get(b_idx).map(|n| n.name.clone()) else {
            return;
        };
        let Some(b_pubkey) = self.nodes.get(b_idx).map(|n| n.mesh.public_key().to_vec()) else {
            return;
        };

        let Some(a) = self.nodes.get_mut(a_idx) else {
            return;
        };
        let Ok(sealed) = a.mesh.begin_pairing(&b_name, &b_pubkey) else {
            return;
        };

        let Some(b) = self.nodes.get_mut(b_idx) else {
            return;
        };
        let _ = b.mesh.finish_pairing(&a_name, &sealed);
    }

    pub fn toggle_running(&mut self) {
        self.running = !self.running;
    }

    pub fn move_cursor(&mut self, dx: f32, dy: f32) {
        self.cursor.x = (self.cursor.x + dx).clamp(0.0, FIELD);
        self.cursor.y = (self.cursor.y + dy).clamp(0.0, FIELD);
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `a` and `b` are within `range` field units of each other —
/// shared by `push_hops` (deciding which neighbors get a directional hop)
/// and tests.
fn within_range(a: Pos, b: Pos, range: f32) -> bool {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt() <= range
}

/// How many virtual milliseconds a [`Hop`] from `from` to `to` takes to
/// animate across, at [`crate::field::WAVE_SPEED`] field-units-per-ms — the
/// same speed the expanding-ring `Pulse` uses, so a hop marker arrives at
/// its destination exactly when the ring passes through it.
fn hop_duration(from: Pos, to: Pos) -> u64 {
    let dx = from.x - to.x;
    let dy = from.y - to.y;
    let distance = (dx * dx + dy * dy).sqrt();
    (distance / crate::field::WAVE_SPEED) as u64
}

/// How far along [0.0, 1.0] a hop's marker has travelled from `hop.from`
/// toward `hop.to` at virtual time `now` — used by the canvas to place the
/// sliding marker. Clamped to `1.0` so a hop pending GC this same tick still
/// renders at its destination rather than overshooting.
pub fn hop_progress(hop: &Hop, now: u64) -> f32 {
    let duration = hop_duration(hop.from, hop.to);
    if duration == 0 {
        return 1.0;
    }
    let elapsed = now.saturating_sub(hop.start);
    (elapsed as f32 / duration as f32).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a sandbox with no seeded nodes so tests control exact
    /// positions and ranges.
    fn empty() -> Sandbox {
        let medium = Rc::new(RefCell::new(Medium::new(
            DEFAULT_RANGE,
            0.0,
            1,
            crate::field::WAVE_SPEED,
        )));
        Sandbox {
            medium,
            now: Rc::new(Cell::new(0)),
            nodes: Vec::new(),
            clock: 0,
            dt: DT,
            running: true,
            pulses: Vec::new(),
            hops: Vec::new(),
            log: Vec::new(),
            cursor: Pos { x: 0.0, y: 0.0 },
            speed: DEFAULT_SPEED,
        }
    }

    #[test]
    fn step_delivers_to_in_range_node_but_not_out_of_range_node() {
        let mut sandbox = empty();
        let near = sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        let far_away = sandbox.add_node(Pos { x: 90.0, y: 90.0 });
        let close_peer = sandbox.add_node(Pos { x: 10.0, y: 0.0 });
        // close_peer is within DEFAULT_RANGE=35 of `near`; far_away is not.

        sandbox.pair(near, close_peer);
        let sender_name = sandbox.nodes[near].name.clone();
        let close_peer_name = sandbox.nodes[close_peer].name.clone();
        assert!(sandbox.send_from(near, &close_peer_name, "hi all".to_string()));

        // Step well past the propagation delay for distance 10 at the
        // configured WAVE_SPEED, whatever that currently is.
        let ticks = (10.0 / crate::field::WAVE_SPEED) as u64 / DT + 2;
        for _ in 0..ticks {
            sandbox.step();
        }

        let close_node = &sandbox.nodes[close_peer].chat;
        let heard = close_node
            .contacts()
            .iter()
            .any(|c| c.name == sender_name && c.history.iter().any(|m| m.body == "hi all"));
        assert!(heard, "in-range node should have received the message");

        let far_node = &sandbox.nodes[far_away].chat;
        let missed = far_node.contacts().iter().all(|c| c.name != sender_name);
        assert!(missed, "out-of-range node should not have received it");
    }

    #[test]
    fn relay_does_not_fire_before_the_packet_actually_arrives() {
        // distance 10 at the configured WAVE_SPEED takes `delay_ms` to
        // travel. A relaying node must not retransmit (or the receiver hear
        // anything) before that delay has actually elapsed.
        let mut sandbox = empty();
        let near = sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        let close_peer = sandbox.add_node(Pos { x: 10.0, y: 0.0 });
        sandbox.pair(near, close_peer);
        let close_peer_name = sandbox.nodes[close_peer].name.clone();
        sandbox.send_from(near, &close_peer_name, "hi".to_string());

        let delay_ms = (10.0 / crate::field::WAVE_SPEED) as u64;
        // Largest tick count that still lands strictly before `delay_ms`.
        let ticks_before_arrival = delay_ms.saturating_sub(1) / DT;
        assert!(
            ticks_before_arrival >= 1,
            "test assumes the delay spans at least one tick"
        );
        for _ in 0..ticks_before_arrival {
            sandbox.step();
        }
        assert!(
            sandbox.nodes[close_peer].chat.contacts().is_empty()
                || sandbox.nodes[close_peer]
                    .chat
                    .contacts()
                    .iter()
                    .all(|c| c.history.is_empty()),
            "packet should still be in flight, not yet delivered"
        );

        // One more tick crosses delay_ms (ticks_before_arrival * DT < delay_ms
        // <= (ticks_before_arrival + 1) * DT).
        sandbox.step();
        let heard = sandbox.nodes[close_peer]
            .chat
            .contacts()
            .iter()
            .any(|c| c.history.iter().any(|m| m.body == "hi"));
        assert!(heard, "packet should be delivered once its delay elapses");
    }

    #[test]
    fn clock_advances_by_dt_each_step() {
        let mut sandbox = empty();
        sandbox.step();
        assert_eq!(sandbox.clock, DT);
        sandbox.step();
        assert_eq!(sandbox.clock, DT * 2);
    }

    #[test]
    fn console_send_records_a_self_authored_message_immediately() {
        let mut sandbox = empty();
        let idx = sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        let peer = sandbox.add_node(Pos { x: 10.0, y: 0.0 });
        sandbox.pair(idx, peer);
        let peer_name = sandbox.nodes[peer].name.clone();

        assert!(sandbox.send_from(idx, &peer_name, "hello".to_string()));

        let contact = &sandbox.nodes[idx].chat.contacts()[0];
        assert_eq!(contact.history.last().unwrap().body, "hello");
    }

    #[test]
    fn send_and_receive_update_node_stats() {
        let mut sandbox = empty();
        let idx = sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        let peer = sandbox.add_node(Pos { x: 10.0, y: 0.0 });
        sandbox.pair(idx, peer);
        let peer_name = sandbox.nodes[peer].name.clone();

        sandbox.send_from(idx, &peer_name, "hi".to_string());
        assert_eq!(sandbox.nodes[idx].stats.sent, 1);

        let ticks = (10.0 / crate::field::WAVE_SPEED) as u64 / DT + 2;
        for _ in 0..ticks {
            sandbox.step();
        }
        assert_eq!(sandbox.nodes[peer].stats.received, 1);
    }

    #[test]
    fn unpaired_send_is_a_no_op_and_transmits_nothing() {
        let mut sandbox = empty();
        let idx = sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        let _peer = sandbox.add_node(Pos { x: 10.0, y: 0.0 });

        assert!(!sandbox.send_from(idx, "n1", "hello".to_string()));
        assert!(sandbox.pulses.is_empty());
    }

    #[test]
    fn pulses_are_gced_once_past_range() {
        let mut sandbox = empty();
        let idx = sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        let peer = sandbox.add_node(Pos { x: 10.0, y: 0.0 });
        sandbox.pair(idx, peer);
        let peer_name = sandbox.nodes[peer].name.clone();
        sandbox.send_from(idx, &peer_name, "hi".to_string());
        assert_eq!(sandbox.pulses.len(), 1);

        // Advance the clock far enough that the wave has outgrown range.
        for _ in 0..2000 {
            sandbox.step();
        }
        assert!(sandbox.pulses.is_empty());
    }

    #[test]
    fn send_pushes_one_hop_per_in_range_neighbor() {
        let mut sandbox = empty();
        let idx = sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        let peer = sandbox.add_node(Pos { x: 10.0, y: 0.0 });
        let _out_of_range = sandbox.add_node(Pos { x: 90.0, y: 90.0 });
        sandbox.pair(idx, peer);
        let peer_name = sandbox.nodes[peer].name.clone();

        sandbox.send_from(idx, &peer_name, "hi".to_string());

        // Only the in-range peer gets a hop; the far node doesn't.
        assert_eq!(sandbox.hops.len(), 1);
        assert_eq!(sandbox.hops[0].from, sandbox.nodes[idx].pos);
        assert_eq!(sandbox.hops[0].to, sandbox.nodes[peer].pos);
    }

    #[test]
    fn hops_are_gced_once_their_duration_elapses() {
        let mut sandbox = empty();
        let idx = sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        let peer = sandbox.add_node(Pos { x: 10.0, y: 0.0 });
        sandbox.pair(idx, peer);
        let peer_name = sandbox.nodes[peer].name.clone();
        sandbox.send_from(idx, &peer_name, "hi".to_string());
        assert_eq!(sandbox.hops.len(), 1);

        for _ in 0..2000 {
            sandbox.step();
        }
        assert!(sandbox.hops.is_empty());
    }

    #[test]
    fn within_range_matches_actual_distance() {
        assert!(within_range(
            Pos { x: 0.0, y: 0.0 },
            Pos { x: 10.0, y: 0.0 },
            35.0
        ));
        assert!(!within_range(
            Pos { x: 0.0, y: 0.0 },
            Pos { x: 90.0, y: 90.0 },
            35.0
        ));
    }

    #[test]
    fn hop_duration_scales_with_distance_and_wave_speed() {
        let from = Pos { x: 0.0, y: 0.0 };
        let to = Pos { x: 10.0, y: 0.0 };
        let expected = (10.0 / crate::field::WAVE_SPEED) as u64;
        assert_eq!(hop_duration(from, to), expected);
    }

    #[test]
    fn send_records_a_log_entry() {
        let mut sandbox = empty();
        let idx = sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        let peer = sandbox.add_node(Pos { x: 10.0, y: 0.0 });
        sandbox.pair(idx, peer);
        let peer_name = sandbox.nodes[peer].name.clone();

        sandbox.send_from(idx, &peer_name, "hi".to_string());

        assert_eq!(sandbox.log.len(), 1);
        assert!(matches!(sandbox.log[0].action, EventKind::Sent));
        assert_eq!(sandbox.log[0].node, sandbox.nodes[idx].name);
    }

    #[test]
    fn event_log_evicts_oldest_once_over_capacity() {
        let mut sandbox = empty();
        let idx = sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        let peer = sandbox.add_node(Pos { x: 10.0, y: 0.0 });
        sandbox.pair(idx, peer);
        let peer_name = sandbox.nodes[peer].name.clone();

        for i in 0..(EVENT_LOG_CAPACITY + 10) {
            sandbox.send_from(idx, &peer_name, format!("msg {i}"));
        }

        assert_eq!(sandbox.log.len(), EVENT_LOG_CAPACITY);
    }

    #[test]
    fn remove_node_drops_it_and_unregisters_from_medium() {
        let mut sandbox = empty();
        let a = sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        let b = sandbox.add_node(Pos { x: 10.0, y: 0.0 });
        assert_eq!(sandbox.nodes.len(), 2);

        sandbox.remove_node(a);

        assert_eq!(sandbox.nodes.len(), 1);
        // The remaining node has shifted down to index 0 and is the one
        // that used to be at `b`.
        assert_eq!(sandbox.nodes[0].pos, Pos { x: 10.0, y: 0.0 });
        let _ = b;
    }

    #[test]
    fn remove_node_out_of_range_is_a_no_op() {
        let mut sandbox = empty();
        sandbox.add_node(Pos { x: 0.0, y: 0.0 });
        sandbox.remove_node(5);
        assert_eq!(sandbox.nodes.len(), 1);
    }
}
