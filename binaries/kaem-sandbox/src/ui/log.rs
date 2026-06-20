//! The event-log panel: a chronological, scrollable trace of every send /
//! relay / receive across the whole sandbox, independent of any one node's
//! chat history. Clicking an entry opens the packet inspector for that
//! entry's frame, reusing the same `inspecting` state the canvas's
//! pulse/hop click-to-inspect writes to.

use std::rc::Rc;

use egui::{RichText, ScrollArea, Ui};

use crate::sandbox::Sandbox;
use crate::theme;

/// Draw the right-docked, collapsible event-log panel. `inspecting` is
/// `app.rs`'s "currently inspected frame" state — clicking a row here sets
/// it exactly the way clicking a pulse/hop on the canvas does.
pub fn render_log_panel(ui: &mut Ui, sandbox: &Sandbox, inspecting: &mut Option<Rc<Vec<u8>>>) {
    egui::Panel::right("event_log")
        .resizable(true)
        .default_size(260.0)
        .show_inside(ui, |ui| {
            ui.label(RichText::new("event log").color(theme::ME).strong());
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
                            .selectable_label(false, RichText::new(line).color(theme::TEXT))
                            .clicked()
                        {
                            *inspecting = Some(entry.frame.clone());
                        }
                    }
                });
        });
}
