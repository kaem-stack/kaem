//! Per-node chat windows: replaces the old ratatui console popup with a
//! native `egui::Window` per attached node, so opening a node reads as
//! "open this node's own app" rather than a modal overlay.
//!
//! The window is a contact list on the left (every paired peer — pairing is
//! the only way a peer appears here, no typing a name) and the selected
//! contact's conversation on the right, like any ordinary chat app.

use egui::{Context, RichText, ScrollArea, TextEdit};

use kaem_mesh::MeshNode;
use kaem_node::Author;

use crate::theme;

/// Per-node UI state that doesn't belong in the engine: whether the window
/// is open, what's currently typed into its send bar, and which contact is
/// selected. Keyed by node index in `app.rs`.
#[derive(Default)]
pub struct ChatState {
    pub open: bool,
    pub draft: String,
    pub selected: Option<String>,
}

/// Draw `node`'s chat window if `state.open`. Returns the message body to
/// send to the selected contact, if the user submitted one this frame
/// (Enter or the Send button).
pub fn show(ctx: &Context, node: &MeshNode, state: &mut ChatState) -> Option<String> {
    if !state.open {
        return None;
    }

    let mut open = state.open;
    let mut submitted = None;

    egui::Window::new(node.callsign())
        .open(&mut open)
        .resizable(true)
        .default_width(420.0)
        .default_height(320.0)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(120.0);
                    render_contacts(ui, node, state);
                });
                ui.separator();
                ui.vertical(|ui| {
                    submitted = render_conversation(ui, node, state);
                });
            });
        });

    state.open = open;
    submitted
}

fn render_contacts(ui: &mut egui::Ui, node: &MeshNode, state: &mut ChatState) {
    ui.label(RichText::new("contacts").color(theme::META));
    ui.separator();

    let mut peers = node.paired_peers();
    peers.sort();

    if peers.is_empty() {
        ui.label(RichText::new("none yet — use \"pair\"").color(theme::META));
        return;
    }

    for peer in peers {
        let selected = state.selected.as_deref() == Some(peer.as_str());
        if ui.selectable_label(selected, &peer).clicked() {
            state.selected = Some(peer);
        }
    }
}

fn render_conversation(
    ui: &mut egui::Ui,
    node: &MeshNode,
    state: &mut ChatState,
) -> Option<String> {
    let Some(selected) = state.selected.clone() else {
        ui.label(RichText::new("select a contact").color(theme::META));
        return None;
    };

    render_log(ui, node, &selected);
    ui.separator();
    render_input(ui, state)
}

fn render_log(ui: &mut egui::Ui, node: &MeshNode, selected: &str) {
    let history = node
        .contacts()
        .iter()
        .find(|c| c.name == selected)
        .map(|c| c.history.as_slice())
        .unwrap_or(&[]);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .max_height(220.0)
        .show(ui, |ui| {
            for message in history {
                let (prefix, color) = match message.author {
                    Author::Me => ("you".to_string(), theme::ME),
                    Author::Them => (selected.to_string(), theme::THEM),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{prefix}:")).color(color));
                    ui.label(RichText::new(&message.body).color(theme::TEXT));
                });
            }
        });
}

fn render_input(ui: &mut egui::Ui, state: &mut ChatState) -> Option<String> {
    let mut submitted = None;
    ui.horizontal(|ui| {
        let edit = TextEdit::singleline(&mut state.draft)
            .desired_width(ui.available_width() - 60.0)
            .hint_text("send>");
        let response = ui.add(edit);
        let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let clicked = ui.button(RichText::new("send").color(theme::ME)).clicked();

        if (enter_pressed || clicked) && !state.draft.trim().is_empty() {
            submitted = Some(std::mem::take(&mut state.draft));
        }
    });
    submitted
}
