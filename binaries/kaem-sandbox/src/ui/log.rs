//! The event-log panel: a chronological, scrollable trace of every send /
//! relay / receive across the whole sandbox, independent of any one node's
//! chat history. Clicking an entry opens the packet inspector for that
//! entry's frame, reusing the same `inspecting` state the canvas's
//! pulse/hop click-to-inspect writes to.

use std::rc::Rc;

use egui::{RichText, ScrollArea, Ui};

use crate::sandbox::{EventKind, Sandbox};
use crate::theme;

/// Draw the right-docked, collapsible event-log panel. `inspecting` is
/// `app.rs`'s "currently inspected frame" state — clicking a row here sets
/// it exactly the way clicking a pulse/hop on the canvas does. `sandbox` is
/// borrowed mutably only so the panel's "clear" button can drain the log
/// in place.
pub fn render_log_panel(ui: &mut Ui, sandbox: &mut Sandbox, inspecting: &mut Option<Rc<Vec<u8>>>) {
    egui::Panel::right("event_log")
        .resizable(true)
        .default_size(280.0)
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("event log").color(theme::ME).strong());
                ui.label(RichText::new(format!("({})", sandbox.log.len())).color(theme::META));
                if ui.small_button("clear").clicked() {
                    sandbox.log.clear();
                }
            });
            ui.separator();

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for entry in &sandbox.log {
                        let line = format!(
                            "{:>7}ms  {:<8} {}",
                            entry.clock,
                            entry.node,
                            entry.action.label()
                        );
                        if ui
                            .selectable_label(
                                false,
                                RichText::new(line).color(event_color(entry.action)),
                            )
                            .clicked()
                        {
                            *inspecting = Some(entry.frame.clone());
                        }
                    }
                });
        });
}

/// Color an event-log row by its kind so the trace reads at a glance:
/// outgoing traffic in the accent color, relays muted, inbound in plain text.
fn event_color(kind: EventKind) -> egui::Color32 {
    match kind {
        EventKind::Sent => theme::ME,
        EventKind::Relayed => theme::THEM,
        EventKind::Received => theme::TEXT,
    }
}
