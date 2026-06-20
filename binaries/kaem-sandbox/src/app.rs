//! The `eframe::App` implementation: owns the `Sandbox` engine plus the
//! UI-only state the engine doesn't need to know about (per-node chat window
//! state, the add-node range control, the packet inspector), and drives one
//! `step()` per frame while running.

use std::rc::Rc;
use std::time::Duration;

use eframe::CreationContext;
use egui::{Context, RichText, Ui};

use crate::field::{
    MAX_ZOOM, MIN_ZOOM, View, field_radius_to_screen, field_to_screen, screen_to_field, zoom_at,
};
use crate::frame_info::{self, DecodedFrame};
use crate::sandbox::Sandbox;
use crate::theme;
use crate::ui::chat::{self, ChatState};
use crate::ui::field::{self as canvas_field, Canvas, CanvasClick, CanvasNode};
use crate::ui::log::render_log_panel;
use crate::ui::nodes::{self, NodeAction};

const CURSOR_STEP: f32 = 2.0;

/// Preset playback-speed multipliers offered in the top bar.
const SPEED_PRESETS: [f32; 4] = [0.5, 1.0, 2.0, 4.0];

pub struct SandboxApp {
    sandbox: Sandbox,
    /// Per-node chat window state, indexed in lockstep with `sandbox.nodes`.
    chats: Vec<ChatState>,
    /// Whether the pairing dialog is open.
    pairing_dialog_open: bool,
    /// The two nodes currently picked in the pairing dialog's dropdowns.
    pairing_a: Option<usize>,
    pairing_b: Option<usize>,
    /// The node currently being dragged on the canvas, if any.
    dragging: Option<usize>,
    /// In-progress drag-rectangle multi-select: the screen-space anchor
    /// point the drag started from. `None` when not selecting.
    select_anchor: Option<egui::Pos2>,
    /// Indices of nodes currently selected via the drag-rectangle.
    selected: Vec<usize>,
    /// The frame currently shown in the packet inspector, if any.
    inspecting: Option<Rc<Vec<u8>>>,
    /// The canvas's zoom/pan window onto the field — UI-only, the engine
    /// has no notion of a viewport.
    view: View,
}

impl SandboxApp {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        cc.egui_ctx.set_global_style(theme::style());
        let sandbox = Sandbox::new();
        let chats = sandbox.nodes.iter().map(|_| ChatState::default()).collect();
        Self {
            sandbox,
            chats,
            pairing_dialog_open: false,
            pairing_a: None,
            pairing_b: None,
            dragging: None,
            select_anchor: None,
            selected: Vec::new(),
            inspecting: None,
            view: View::default(),
        }
    }

    /// Keep `chats` the same length as `sandbox.nodes` after a node is added.
    fn sync_chat_state(&mut self) {
        while self.chats.len() < self.sandbox.nodes.len() {
            self.chats.push(ChatState::default());
        }
    }

    /// Discard the current sandbox and every piece of UI-only state that
    /// referenced it, replacing it with a freshly seeded one. Local-only
    /// state (no file or process involved), so this is a plain rebuild
    /// rather than anything needing confirmation.
    fn reset(&mut self) {
        self.sandbox = Sandbox::new();
        self.chats = self
            .sandbox
            .nodes
            .iter()
            .map(|_| ChatState::default())
            .collect();
        self.pairing_dialog_open = false;
        self.pairing_a = None;
        self.pairing_b = None;
        self.dragging = None;
        self.select_anchor = None;
        self.selected.clear();
        self.inspecting = None;
        self.view = View::default();
    }

    /// Remove the node at `idx` from the engine and every piece of lockstep
    /// UI state: its chat window and its entry (if any) in `selected`,
    /// shifting indices above `idx` down by one to match `Vec::remove`.
    fn remove_node(&mut self, idx: usize) {
        self.sandbox.remove_node(idx);
        if idx < self.chats.len() {
            self.chats.remove(idx);
        }
        self.selected.retain(|&i| i != idx);
        for i in self.selected.iter_mut() {
            if *i > idx {
                *i -= 1;
            }
        }
    }
}

impl eframe::App for SandboxApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        if self.sandbox.running {
            // `step()` itself always advances by exactly `dt` — the speed
            // control instead steps multiple times per real frame at higher
            // multipliers, leaving `step()`'s own per-call semantics (used by
            // tests and the manual "step" button) unchanged.
            let steps = self.sandbox.speed.max(0.0).round().max(1.0) as usize;
            for _ in 0..steps {
                self.sandbox.step();
            }
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }

        self.top_bar(ui);
        self.bottom_bar(ui);
        self.nodes_panel(ui);
        render_log_panel(ui, &mut self.sandbox, &mut self.inspecting);
        self.central_canvas(ui);
        self.chat_windows(ui.ctx());
        self.pairing_dialog(ui.ctx());
        self.packet_inspector(ui.ctx());
    }
}

impl SandboxApp {
    fn top_bar(&mut self, ui: &mut Ui) {
        egui::Panel::top("top_bar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("kaem sandbox").color(theme::ME).strong());
                ui.separator();

                let label = if self.sandbox.running { "pause" } else { "run" };
                if ui.button(label).clicked() {
                    self.sandbox.toggle_running();
                }
                if ui
                    .add_enabled(!self.sandbox.running, egui::Button::new("step"))
                    .clicked()
                {
                    self.sandbox.step();
                }
                if ui.button("reset").clicked() {
                    self.reset();
                }

                ui.separator();
                ui.label(RichText::new("speed").color(theme::META));
                for preset in SPEED_PRESETS {
                    let label = format!("{preset}x");
                    let active = (self.sandbox.speed - preset).abs() < f32::EPSILON;
                    if ui.selectable_label(active, label).clicked() {
                        self.sandbox.speed = preset;
                    }
                }

                ui.separator();
                if ui.button("add node").clicked() {
                    let pos = self.sandbox.cursor;
                    self.sandbox.add_node(pos);
                    self.sync_chat_state();
                }
                if ui.button("pair...").clicked() {
                    self.pairing_dialog_open = true;
                    self.pairing_a = None;
                    self.pairing_b = None;
                }

                ui.separator();
                let mut range = self.sandbox.medium.borrow().range();
                if ui
                    .add(egui::Slider::new(&mut range, 1.0..=150.0).text("range (m)"))
                    .changed()
                {
                    self.sandbox.medium.borrow_mut().set_range(range);
                }
                let mut loss = self.sandbox.medium.borrow().loss();
                if ui
                    .add(egui::Slider::new(&mut loss, 0.0..=1.0).text("loss"))
                    .changed()
                {
                    self.sandbox.medium.borrow_mut().set_loss(loss);
                }

                ui.separator();
                if ui.small_button("-").clicked() {
                    self.view.zoom = (self.view.zoom / 1.25).clamp(MIN_ZOOM, MAX_ZOOM);
                }
                ui.label(RichText::new(format!("{:.1}x", self.view.zoom)).color(theme::META));
                if ui.small_button("+").clicked() {
                    self.view.zoom = (self.view.zoom * 1.25).clamp(MIN_ZOOM, MAX_ZOOM);
                }
                if ui.button("reset view").clicked() {
                    self.view = View::default();
                }

                if !self.selected.is_empty() {
                    ui.separator();
                    ui.label(
                        RichText::new(format!("{} selected", self.selected.len()))
                            .color(theme::META),
                    );
                    if ui.button("pair selected").clicked() {
                        self.pair_selected();
                    }
                    if ui.button("remove selected").clicked() {
                        self.remove_selected();
                    }
                    if ui.button("clear selection").clicked() {
                        self.selected.clear();
                    }
                }
            });
        });
    }

    /// Draw the left-docked node roster and resolve whatever quick action
    /// the operator clicked — opening a chat, jumping into the pairing
    /// dialog pre-filled with this node, or removing it outright.
    fn nodes_panel(&mut self, ui: &mut Ui) {
        let action = nodes::render_nodes_panel(ui, &self.sandbox, &self.selected);
        match action {
            Some(NodeAction::Open(idx)) => {
                if let Some(state) = self.chats.get_mut(idx) {
                    state.open = true;
                }
            }
            Some(NodeAction::Pair(idx)) => {
                self.pairing_dialog_open = true;
                self.pairing_a = Some(idx);
                self.pairing_b = None;
            }
            Some(NodeAction::Remove(idx)) => self.remove_node(idx),
            None => {}
        }
    }

    /// Pairwise-pair every currently selected node with every other selected
    /// node.
    fn pair_selected(&mut self) {
        for i in 0..self.selected.len() {
            for j in (i + 1)..self.selected.len() {
                self.sandbox.pair(self.selected[i], self.selected[j]);
            }
        }
    }

    /// Remove every currently selected node from the sandbox, highest index
    /// first so earlier indices in `self.selected` stay valid as we go.
    fn remove_selected(&mut self) {
        let mut indices = self.selected.clone();
        indices.sort_unstable();
        indices.dedup();
        for idx in indices.into_iter().rev() {
            self.remove_node(idx);
        }
    }

    fn bottom_bar(&self, ui: &mut Ui) {
        egui::Panel::bottom("bottom_bar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let state = if self.sandbox.running { "run" } else { "pause" };
                ui.label(RichText::new(format!("t={}ms", self.sandbox.clock)).color(theme::META));
                ui.separator();
                ui.label(RichText::new(format!("[{state}]")).color(theme::META));
                ui.separator();

                let nodes = self.sandbox.nodes.len();
                let links = self.sandbox.medium.borrow().reachable().len();
                let in_flight = self.sandbox.pulses.len();
                let (sent, received, relayed) = self.sandbox.nodes.iter().fold(
                    (0u64, 0u64, 0u64),
                    |(s, r, x), n| (s + n.stats.sent, r + n.stats.received, x + n.stats.relayed),
                );
                ui.label(
                    RichText::new(format!(
                        "nodes {nodes}  links {links}  in-flight {in_flight}  sent {sent}  recv {received}  relay {relayed}"
                    ))
                    .color(theme::META),
                );
                ui.separator();

                let hint = "click a node to open its chat; drag empty field to select; click a wave/hop to inspect";
                ui.label(RichText::new(hint).color(theme::META));
            });
        });
    }

    fn central_canvas(&mut self, ui: &mut Ui) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            let canvas_nodes: Vec<CanvasNode> = self
                .sandbox
                .nodes
                .iter()
                .enumerate()
                .map(|(i, n)| CanvasNode {
                    name: &n.name,
                    pos: n.pos,
                    emphasized: self.chats.get(i).is_some_and(|c| c.open)
                        || self.selected.contains(&i),
                    paired: !n.mesh.paired_peers().is_empty(),
                })
                .collect();

            let medium = self.sandbox.medium.borrow();
            let canvas = Canvas {
                medium: &medium,
                nodes: &canvas_nodes,
                pulses: &self.sandbox.pulses,
                hops: &self.sandbox.hops,
                now: self.sandbox.clock,
                cursor: self.sandbox.cursor,
                view: self.view,
            };
            let output = canvas.show(ui);
            drop(medium);

            handle_zoom_and_pan(ui, &output.response, output.inner, &mut self.view);

            let pointer = output.response.interact_pointer_pos();

            if self.dragging.is_none()
                && self.select_anchor.is_none()
                && output.response.drag_started()
                && let Some(point) = pointer
            {
                let pos = screen_to_field(output.inner, point, self.view);
                self.dragging =
                    canvas_field::nearest_node(&canvas_nodes, pos, canvas_field::HIT_THRESHOLD);
                if self.dragging.is_none() {
                    // Drag started on bare field — begin a multi-select
                    // rectangle instead of moving a node.
                    self.select_anchor = Some(point);
                }
            }

            if let Some(idx) = self.dragging {
                if let Some(point) = pointer {
                    self.sandbox
                        .move_node(idx, screen_to_field(output.inner, point, self.view));
                }
                if output.response.drag_stopped() {
                    self.dragging = None;
                }
            } else if let Some(anchor) = self.select_anchor {
                if let Some(point) = pointer {
                    let rect = egui::Rect::from_two_pos(anchor, point);
                    output.response.ctx.debug_painter().rect_stroke(
                        rect,
                        0.0,
                        egui::Stroke::new(1.0, theme::ME),
                        egui::StrokeKind::Inside,
                    );
                }
                if output.response.drag_stopped() {
                    if let Some(point) = pointer {
                        let rect = egui::Rect::from_two_pos(anchor, point);
                        self.selected = canvas_nodes
                            .iter()
                            .enumerate()
                            .filter(|(_, n)| {
                                rect.contains(field_to_screen(output.inner, n.pos, self.view))
                            })
                            .map(|(i, _)| i)
                            .collect();
                    }
                    self.select_anchor = None;
                }
            } else {
                match output.click {
                    Some(CanvasClick::Node(idx)) => {
                        if let Some(state) = self.chats.get_mut(idx) {
                            state.open = true;
                        }
                    }
                    Some(CanvasClick::Field(pos)) => {
                        self.sandbox.cursor = pos;
                        self.selected.clear();
                    }
                    Some(CanvasClick::Frame(frame)) => {
                        self.inspecting = Some(frame);
                    }
                    None => {}
                }
            }

            ui.input(|i| {
                if i.key_pressed(egui::Key::ArrowUp) {
                    self.sandbox.move_cursor(0.0, -CURSOR_STEP);
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    self.sandbox.move_cursor(0.0, CURSOR_STEP);
                }
                if i.key_pressed(egui::Key::ArrowLeft) {
                    self.sandbox.move_cursor(-CURSOR_STEP, 0.0);
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    self.sandbox.move_cursor(CURSOR_STEP, 0.0);
                }
            });
        });
    }

    fn chat_windows(&mut self, ctx: &Context) {
        for idx in 0..self.sandbox.nodes.len() {
            let Some(state) = self.chats.get_mut(idx) else {
                continue;
            };
            if !state.open {
                continue;
            }
            let node = &self.sandbox.nodes[idx];
            if let Some(body) = chat::show(ctx, node, state)
                && let Some(to) = state.selected.clone()
            {
                self.sandbox.send_from(idx, &to, body);
            }
        }
    }

    /// A dialog with two node pickers and a confirm button — the only way to
    /// pair two nodes, replacing the earlier click-two-nodes-on-canvas flow.
    fn pairing_dialog(&mut self, ctx: &Context) {
        if !self.pairing_dialog_open {
            return;
        }

        let mut open = self.pairing_dialog_open;
        egui::Window::new("pair nodes")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::ComboBox::from_label("node 1")
                    .selected_text(self.node_label(self.pairing_a))
                    .show_ui(ui, |ui| {
                        for (i, n) in self.sandbox.nodes.iter().enumerate() {
                            ui.selectable_value(&mut self.pairing_a, Some(i), &n.name);
                        }
                    });

                egui::ComboBox::from_label("node 2")
                    .selected_text(self.node_label(self.pairing_b))
                    .show_ui(ui, |ui| {
                        for (i, n) in self.sandbox.nodes.iter().enumerate() {
                            ui.selectable_value(&mut self.pairing_b, Some(i), &n.name);
                        }
                    });

                ui.separator();

                let ready =
                    matches!((self.pairing_a, self.pairing_b), (Some(a), Some(b)) if a != b);
                ui.horizontal(|ui| {
                    if ui.add_enabled(ready, egui::Button::new("pair")).clicked() {
                        if let (Some(a), Some(b)) = (self.pairing_a, self.pairing_b) {
                            self.sandbox.pair(a, b);
                        }
                        self.pairing_dialog_open = false;
                    }
                    if ui.button("cancel").clicked() {
                        self.pairing_dialog_open = false;
                    }
                });
            });

        self.pairing_dialog_open &= open;
    }

    fn node_label(&self, idx: Option<usize>) -> String {
        idx.and_then(|i| self.sandbox.nodes.get(i))
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "—".to_string())
    }

    /// A read-only window decoding `self.inspecting`'s frame via
    /// `frame_info::decode_frame` — the packet inspector. Opened by clicking
    /// a pulse ring; closing it just clears `inspecting`, no engine state
    /// involved.
    fn packet_inspector(&mut self, ctx: &Context) {
        let Some(frame) = self.inspecting.clone() else {
            return;
        };

        let mut open = true;
        egui::Window::new("packet inspector")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| match frame_info::decode_frame(&frame) {
                DecodedFrame::Envelope {
                    chatroom_id,
                    message_id,
                    ttl,
                    ciphertext_len,
                } => {
                    ui.label(RichText::new("mesh envelope (KE)").color(theme::ME));
                    ui.separator();
                    ui.label(format!("chatroom_id: {chatroom_id}"));
                    ui.label(format!("message_id:  {message_id}"));
                    ui.label(format!("ttl:         {ttl}"));
                    ui.label(format!("ciphertext:  {ciphertext_len} bytes (opaque)"));
                }
                DecodedFrame::Wire(message) => {
                    ui.label(RichText::new("chat wire frame (KM)").color(theme::ME));
                    ui.separator();
                    ui.label(format!("from: {}", message.from));
                    ui.label(format!("to:   {}", message.to));
                    ui.label(format!("body: {}", message.body));
                }
                DecodedFrame::Unknown => {
                    ui.label(RichText::new("unrecognized frame").color(theme::META));
                    ui.label(format!("{} bytes", frame.len()));
                }
            });

        if !open {
            self.inspecting = None;
        }
    }
}

/// Mouse-wheel zoom (anchored on the pointer, so the field point under the
/// cursor stays put) and middle-button-drag pan, both only while the pointer
/// is over the canvas. A free function (not a `SandboxApp` method) so it can
/// take just `&mut View` — the canvas's per-frame node list otherwise holds
/// an immutable borrow of `self.sandbox` alive across this call.
fn handle_zoom_and_pan(ui: &Ui, response: &egui::Response, inner: egui::Rect, view: &mut View) {
    if !response.hovered() {
        return;
    }
    ui.input(|i| {
        let scroll = i.smooth_scroll_delta.y;
        if scroll.abs() > f32::EPSILON
            && let Some(point) = i.pointer.hover_pos()
        {
            let factor = (1.0 + scroll * 0.0015).clamp(0.1, 10.0);
            *view = zoom_at(inner, *view, point, view.zoom * factor);
        }

        if i.pointer.button_down(egui::PointerButton::Middle) {
            let delta = i.pointer.delta();
            let scale = field_radius_to_screen(inner, 1.0, *view);
            if delta != egui::Vec2::ZERO && scale > 0.0 {
                view.center.x -= delta.x / scale;
                view.center.y -= delta.y / scale;
            }
        }
    });
}
