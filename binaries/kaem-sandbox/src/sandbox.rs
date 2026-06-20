//! `Sandbox` owns every simulated node, the shared RF [`Medium`], and the
//! deterministic tick loop. Rendering-free so the engine logic
//! ([`Sandbox::step`], coordinate-independent message delivery) is directly
//! unit-testable, independent of whatever UI framework drives it.

use std::cell::RefCell;
use std::rc::Rc;

use kaem_link::{Medium, NodeId, Pos, RadioTransport, SimChannel, Transport};
use kaem_mesh::MeshNode;
use kaem_node::{Command, Node};

use crate::field::FIELD;

/// Virtual milliseconds per tick.
pub const DT: u64 = 50;

const DEFAULT_RANGE: f32 = 35.0;
const DEFAULT_LOSS: f32 = 0.0;
const DEFAULT_SEED: u64 = 1;

const NAMES: &[&str] = &[
    "alice", "bob", "carol", "dave", "erin", "frank", "grace", "heidi",
];

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
    pub transport: RadioTransport,
}

/// An expanding RF wave drawn from a transmit, for the canvas animation.
pub struct Pulse {
    pub origin: Pos,
    pub start: u64,
}

pub struct Sandbox {
    pub medium: Rc<RefCell<Medium>>,
    pub nodes: Vec<SimNode>,
    pub clock: u64,
    pub dt: u64,
    pub running: bool,
    pub pulses: Vec<Pulse>,
    pub cursor: Pos,
}

impl Sandbox {
    pub fn new() -> Self {
        let medium = Rc::new(RefCell::new(Medium::new(
            DEFAULT_RANGE,
            DEFAULT_LOSS,
            DEFAULT_SEED,
        )));

        let mut sandbox = Self {
            medium,
            nodes: Vec::new(),
            clock: 0,
            dt: DT,
            running: true,
            pulses: Vec::new(),
            cursor: Pos {
                x: FIELD / 2.0,
                y: FIELD / 2.0,
            },
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
    /// a fresh [`SimChannel`] on the shared medium.
    pub fn add_node(&mut self, pos: Pos) -> usize {
        let id = self.medium.borrow_mut().register(pos);
        let name = NAMES
            .get(self.nodes.len())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("n{}", self.nodes.len()));

        let transport = RadioTransport::new(Box::new(SimChannel::new(id, self.medium.clone())));
        let chat = Node::new(name.clone());

        self.nodes.push(SimNode {
            name,
            id,
            pos,
            chat,
            mesh: MeshNode::new(),
            transport,
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

    /// Advance the simulation by exactly one tick:
    /// 1. advance the clock,
    /// 2. drain each node's transport into `on_frame`, retransmitting any
    ///    relay outbound it returns (flood-relay: a node that can't decrypt
    ///    an envelope still rebroadcasts it with a decremented TTL),
    /// 3. garbage-collect pulses that have outgrown the medium's range.
    pub fn step(&mut self) {
        self.clock += self.dt;

        for node in &mut self.nodes {
            let mut relays = Vec::new();
            while let Ok(Some(frame)) = node.transport.recv() {
                let inbound = node.mesh.on_frame(&frame);
                // Decrypted payload (if any) folds into this node's chat
                // history; the relay (if any) is rebroadcast below.
                if let Some(payload) = inbound.payload {
                    node.chat.on_frame(&payload, self.clock);
                }
                if let Some(relay) = inbound.relay {
                    relays.push(relay);
                }
            }
            for relay in relays {
                let _ = node.transport.send(&relay);
                self.pulses.push(Pulse {
                    origin: node.pos,
                    start: self.clock,
                });
            }
        }

        let range = self.medium.borrow().range();
        let now = self.clock;
        self.pulses
            .retain(|p| (now.saturating_sub(p.start) as f32) * crate::field::WAVE_SPEED <= range);
    }

    /// Send a console message from the node at `idx` to `to` immediately:
    /// seal it for their shared chatroom, transmit the resulting envelope,
    /// and push a wave pulse. Returns `false` (and transmits nothing) if
    /// `idx` and `to` aren't paired — the caller surfaces that as a status
    /// line. Delivery to receivers still happens on the next `step` — that's
    /// the deterministic-sim semantics.
    pub fn send_from(&mut self, idx: usize, to: &str, body: String) -> bool {
        let now = self.clock;
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
        for frame in frames {
            if let Some(envelope) = node.mesh.seal(to, &frame.0) {
                let _ = node.transport.send(&envelope);
                self.pulses.push(Pulse {
                    origin: node.pos,
                    start: now,
                });
                transmitted = true;
            }
        }
        transmitted
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a sandbox with no seeded nodes so tests control exact
    /// positions and ranges.
    fn empty() -> Sandbox {
        let medium = Rc::new(RefCell::new(Medium::new(DEFAULT_RANGE, 0.0, 1)));
        Sandbox {
            medium,
            nodes: Vec::new(),
            clock: 0,
            dt: DT,
            running: true,
            pulses: Vec::new(),
            cursor: Pos { x: 0.0, y: 0.0 },
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

        for _ in 0..5 {
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
}
