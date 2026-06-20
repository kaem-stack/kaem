//! An in-process RF medium: it carries baseband IQ samples between in-memory
//! nodes positioned in a 2D field, instead of over a real socket or radio. A
//! burst is delivered to every other node within `range`, each independently
//! subject to Bernoulli `loss`.
//!
//! It knows nothing of what those samples carry — chat, pairing, anything —
//! only positions and IQ, and nothing of how a real link (`kaem-link`)
//! carries the same samples over UDP or a real SDR; a binary is the one that
//! adapts a [`Medium`] to whatever link-layer seam it needs to satisfy.
//! Deliberately single-threaded: callers drive every node from one tick
//! loop, so `Medium` is shared via `Rc<RefCell<_>>` rather than made `Send`.

use std::collections::{HashMap, VecDeque};

use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;

/// One baseband in-phase/quadrature sample. Mirrors `kaem_link::modem::Iq`'s
/// shape; the two are independent types — a caller adapting this crate to a
/// real link converts between them at the boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Iq {
    pub i: f32,
    pub q: f32,
}

/// A node's location in the simulated field, in arbitrary distance units
/// (meters, conceptually).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
}

/// A handle to a node registered with a [`Medium`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

struct NodeState {
    pos: Pos,
    /// FIFO of bursts in flight to this node, each stamped with the virtual
    /// millisecond it's due to arrive — modeling real propagation delay
    /// rather than instant delivery, so a node can't act on (or relay) a
    /// burst before it has actually "arrived".
    inbox: VecDeque<(u64, Vec<Iq>)>,
}

/// The in-process RF field. Owns every registered node's position and inbox,
/// and decides — deterministically, given a seed — which transmissions reach
/// which nodes.
pub struct Medium {
    nodes: HashMap<NodeId, NodeState>,
    range: f32,
    loss: f32,
    /// Propagation speed in field-units per virtual millisecond — how long a
    /// burst takes to cross the field, used to delay delivery realistically.
    speed: f32,
    rng: StdRng,
    next_id: u32,
}

impl Medium {
    /// `range` is the maximum Euclidean distance at which a node can hear a
    /// transmission; `loss` is the independent per-recipient probability
    /// [0,1] that an in-range burst is dropped anyway; `speed` is the
    /// propagation speed in field-units per virtual millisecond, used to
    /// delay delivery by `distance / speed` rather than delivering instantly.
    /// `seed` makes the loss decisions reproducible.
    pub fn new(range: f32, loss: f32, seed: u64, speed: f32) -> Self {
        Self {
            nodes: HashMap::new(),
            range,
            loss,
            speed,
            rng: StdRng::seed_from_u64(seed),
            next_id: 0,
        }
    }

    /// Register a new node at `pos`, returning its handle.
    pub fn register(&mut self, pos: Pos) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.nodes.insert(
            id,
            NodeState {
                pos,
                inbox: VecDeque::new(),
            },
        );
        id
    }

    /// Move a registered node. A no-op if `id` isn't registered.
    pub fn set_position(&mut self, id: NodeId, pos: Pos) {
        if let Some(node) = self.nodes.get_mut(&id) {
            node.pos = pos;
        }
    }

    /// Drop a node from the field; its pending inbox goes with it.
    pub fn remove(&mut self, id: NodeId) {
        self.nodes.remove(&id);
    }

    /// The field range (max hearing distance).
    pub fn range(&self) -> f32 {
        self.range
    }

    /// Set the field range (max hearing distance).
    pub fn set_range(&mut self, range: f32) {
        self.range = range;
    }

    /// The per-recipient drop probability.
    pub fn loss(&self) -> f32 {
        self.loss
    }

    /// Set the per-recipient drop probability. Clamped to `[0, 1]` since it's
    /// used as a Bernoulli trial probability.
    pub fn set_loss(&mut self, loss: f32) {
        self.loss = loss.clamp(0.0, 1.0);
    }

    /// The current position of a registered node, if it exists.
    pub fn position(&self, id: NodeId) -> Option<Pos> {
        self.nodes.get(&id).map(|n| n.pos)
    }

    /// Deliver `burst` to every other registered node within `range` of
    /// `from`, each independently dropped with probability `loss`, arriving
    /// at `now + distance / speed` rather than instantly — `from` never
    /// receives its own transmission. A no-op if `from` isn't registered
    /// (nowhere to transmit "from").
    pub fn transmit(&mut self, from: NodeId, burst: &[Iq], now: u64) {
        let Some(origin) = self.nodes.get(&from).map(|n| n.pos) else {
            return;
        };

        let in_range: Vec<(NodeId, f32)> = self
            .nodes
            .iter()
            .filter(|&(&id, node)| id != from && within_range(origin, node.pos, self.range))
            .map(|(&id, node)| (id, distance(origin, node.pos)))
            .collect();

        for (id, dist) in in_range {
            let dropped = self.loss > 0.0 && self.rng.random_bool(self.loss as f64);
            if !dropped && let Some(node) = self.nodes.get_mut(&id) {
                let delay = (dist / self.speed) as u64;
                node.inbox.push_back((now + delay, burst.to_vec()));
            }
        }
    }

    /// Pop the next burst delivered to `id` whose arrival time has passed,
    /// if any (FIFO, one burst per call). A burst still in flight (its
    /// arrival time is after `now`) is left queued — it isn't deliverable
    /// yet, so the node must not see or act on it.
    pub fn take_burst(&mut self, id: NodeId, now: u64) -> Option<Vec<Iq>> {
        let node = self.nodes.get_mut(&id)?;
        match node.inbox.front() {
            Some((deliver_at, _)) if *deliver_at <= now => {
                node.inbox.pop_front().map(|(_, burst)| burst)
            }
            _ => None,
        }
    }

    /// Unordered, unique pairs of nodes currently within `range` of each
    /// other — e.g. for a caller that wants to show which nodes can hear each
    /// other.
    pub fn reachable(&self) -> Vec<(NodeId, NodeId)> {
        let mut ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        ids.sort_by_key(|id| id.0);

        let mut pairs = Vec::new();
        for (i, &a) in ids.iter().enumerate() {
            for &b in &ids[i + 1..] {
                let pos_a = self.nodes[&a].pos;
                let pos_b = self.nodes[&b].pos;
                if within_range(pos_a, pos_b, self.range) {
                    pairs.push((a, b));
                }
            }
        }
        pairs
    }
}

/// Whether `b` is within `range` of `a` — compares squared distances so
/// callers (run every tick, for every pair) never pay for a `sqrt` just to
/// answer a yes/no threshold question.
fn within_range(a: Pos, b: Pos, range: f32) -> bool {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy <= range * range
}

/// Euclidean distance between two field positions.
fn distance(a: Pos, b: Pos) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    // A speed fast enough that delay rounds down to 0ms at the short
    // distances most tests use — i.e. "instant" delivery, isolating these
    // tests from the propagation-delay behavior covered separately below.
    const FAST: f32 = 1000.0;

    #[test]
    fn delivers_within_range_but_not_back_to_sender() {
        let mut medium = Medium::new(10.0, 0.0, 1, FAST);
        let a = medium.register(Pos { x: 0.0, y: 0.0 });
        let b = medium.register(Pos { x: 5.0, y: 0.0 });

        let burst = vec![Iq { i: 1.0, q: 0.0 }];
        medium.transmit(a, &burst, 0);

        assert_eq!(medium.take_burst(b, 0).as_deref(), Some(&burst[..]));
        assert_eq!(medium.take_burst(a, 0), None);
    }

    #[test]
    fn out_of_range_delivers_nothing() {
        let mut medium = Medium::new(10.0, 0.0, 1, FAST);
        let a = medium.register(Pos { x: 0.0, y: 0.0 });
        let b = medium.register(Pos { x: 50.0, y: 0.0 });

        medium.transmit(a, &[Iq { i: 1.0, q: 0.0 }], 0);

        assert_eq!(medium.take_burst(b, 0), None);
    }

    #[test]
    fn seeded_loss_is_deterministic() {
        fn run(seed: u64) -> Vec<bool> {
            let mut medium = Medium::new(10.0, 0.5, seed, FAST);
            let a = medium.register(Pos { x: 0.0, y: 0.0 });
            let b = medium.register(Pos { x: 1.0, y: 0.0 });

            let mut delivered = Vec::new();
            for now in 0..50 {
                medium.transmit(a, &[Iq { i: 1.0, q: 0.0 }], now);
                delivered.push(medium.take_burst(b, now).is_some());
            }
            delivered
        }

        let first = run(42);
        let second = run(42);
        assert_eq!(first, second);
        // Sanity: with loss = 0.5 over 50 trials we should see some drops and
        // some deliveries, otherwise the test isn't exercising the rng path.
        assert!(first.iter().any(|&d| d));
        assert!(first.iter().any(|&d| !d));
    }

    #[test]
    fn burst_is_not_deliverable_before_its_propagation_delay_elapses() {
        // speed = 1 unit/ms, distance = 10 units -> 10ms travel time.
        let mut medium = Medium::new(20.0, 0.0, 1, 1.0);
        let a = medium.register(Pos { x: 0.0, y: 0.0 });
        let b = medium.register(Pos { x: 10.0, y: 0.0 });

        medium.transmit(a, &[Iq { i: 1.0, q: 0.0 }], 0);

        assert_eq!(medium.take_burst(b, 5), None, "still in flight at t=5ms");
        assert!(medium.take_burst(b, 10).is_some(), "arrived by t=10ms");
    }
}
