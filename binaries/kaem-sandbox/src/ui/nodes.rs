//! Left-docked node roster: every simulated node with its pairing/stat
//! summary and quick actions, so finding and managing a node doesn't require
//! locating it on the canvas first.

use egui::{RichText, ScrollArea, Ui};

use crate::sandbox::Sandbox;
use crate::theme;

/// What the operator clicked in the roster, if anything — resolved by
/// `app.rs` against UI state this module doesn't own (chat windows, the
/// pairing dialog, node removal).
pub enum NodeAction {
    Open(usize),
    Pair(usize),
    Remove(usize),
}

pub fn render_nodes_panel(
    ui: &mut Ui,
    sandbox: &Sandbox,
    selected: &[usize],
) -> Option<NodeAction> {
    let mut action = None;

    egui::Panel::left("nodes_panel")
        .resizable(true)
        .default_size(210.0)
        .show_inside(ui, |ui| {
            ui.label(RichText::new("nodes").color(theme::ME).strong());
            ui.separator();

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (idx, node) in sandbox.nodes.iter().enumerate() {
                        let paired = node.mesh.paired_peers().len();
                        let is_selected = selected.contains(&idx);

                        ui.separator();
                        let name_color = if is_selected { theme::ME } else { theme::TEXT };
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(">{}", node.name))
                                    .color(name_color)
                                    .strong(),
                            );
                            ui.label(RichText::new(format!("[{paired}p]")).color(theme::META));
                        });
                        ui.label(
                            RichText::new(format!(
                                "tx:{} rx:{} rl:{}",
                                node.stats.sent, node.stats.received, node.stats.relayed
                            ))
                            .color(theme::META),
                        );
                        ui.horizontal(|ui| {
                            if ui.selectable_label(false, "open").clicked() {
                                action = Some(NodeAction::Open(idx));
                            }
                            if ui.selectable_label(false, "pair").clicked() {
                                action = Some(NodeAction::Pair(idx));
                            }
                            if ui.selectable_label(false, "remove").clicked() {
                                action = Some(NodeAction::Remove(idx));
                            }
                        });
                    }

                    if sandbox.nodes.is_empty() {
                        ui.label(RichText::new("no nodes — use \"add node\"").color(theme::META));
                    } else {
                        ui.separator();
                    }
                });
        });

    action
}
