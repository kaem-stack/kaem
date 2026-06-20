//! Adapts `kaem-sim`'s [`Medium`] to `kaem-link`'s [`Channel`] seam. Neither
//! library crate names the other — `kaem-sim` carries its own `Iq` type,
//! `kaem-link` carries its own — so this binary is the only place that
//! converts between them and bridges the two trait surfaces.

use std::cell::{Cell, RefCell};
use std::net::SocketAddr;
use std::rc::Rc;

use kaem_link::{Channel, Iq as LinkIq, TransportError};
use kaem_sim::{Iq as SimIq, Medium, NodeId};

pub struct SimChannelAdapter {
    id: NodeId,
    medium: Rc<RefCell<Medium>>,
    /// The sandbox's virtual clock, shared by every node's adapter — `Medium`
    /// needs "now" to stamp and check propagation delay, but `Channel`'s
    /// `transmit`/`receive` don't carry a time parameter (a real radio
    /// channel doesn't need one; physics handles delay on its own), so
    /// `Sandbox::step` updates this cell and every adapter reads it.
    now: Rc<Cell<u64>>,
}

impl SimChannelAdapter {
    pub fn new(id: NodeId, medium: Rc<RefCell<Medium>>, now: Rc<Cell<u64>>) -> Self {
        Self { id, medium, now }
    }
}

impl Channel for SimChannelAdapter {
    fn transmit(&mut self, samples: &[LinkIq]) -> Result<(), TransportError> {
        let burst: Vec<SimIq> = samples.iter().map(|s| SimIq { i: s.i, q: s.q }).collect();
        self.medium
            .borrow_mut()
            .transmit(self.id, &burst, self.now.get());
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<Vec<LinkIq>>, TransportError> {
        Ok(self
            .medium
            .borrow_mut()
            .take_burst(self.id, self.now.get())
            .map(|burst| {
                burst
                    .into_iter()
                    .map(|s| LinkIq { i: s.i, q: s.q })
                    .collect()
            }))
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        None // the sim has no network address; inherits the Channel default
    }
}
