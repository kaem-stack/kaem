//! egui rendering for the sandbox: the field canvas and per-node chat
//! windows. Everything here is pure presentation over `Sandbox` state — no
//! engine logic lives in this module tree.

pub mod chat;
pub mod field;
pub mod log;
pub mod nodes;
