//! The `eframe::App` implementation: owns the `Sandbox` engine plus the
//! UI-only state the engine doesn't need to know about (per-node chat window
//! state, the add-node range control, the packet inspector), and drives one
//! `step()` per frame while running.

use std::rc::Rc;
use std::time::Duration;

use eframe::CreationContext;
use egui::{Context, RichText, Ui};

use crate::field::screen_to_field;
use crate::frame_info::{self, DecodedFrame};
use crate::sandbox::Sandbox;
use crate::theme;
use crate::ui::chat::{self, ChatState};
use crate::ui::field::{self, Canvas, CanvasClick, CanvasNode};
use crate::ui::log::render_log_panel;

const CURSOR_STEP: f32 = 2.0;

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
    /// The frame currently shown in the packet inspector, if any.
    inspecting: Option<Rc<Vec<u8>>>,
}

impl SandboxApp {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(theme::visuals());
        let sandbox = Sandbox::new();
        let chats = sandbox.nodes.iter().map(|_| ChatState::default()).collect();
        Self {
            sandbox,
            chats,
            pairing_dialog_open: false,
            pairing_a: None,
            pairing_b: None,
            dragging: None,
            inspecting: None,
        }
    }

    /// Keep `chats` the same length as `sandbox.nodes` after a node is added.
    fn sync_chat_state(&mut self) {
        while self.chats.len() < self.sandbox.nodes.len() {
            self.chats.push(ChatState::default());
        }
    }
}

impl eframe::App for SandboxApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        if self.sandbox.running {
            self.sandbox.step();
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }

        self.top_bar(ui);
        self.bottom_bar(ui);
        render_log_panel(ui, &self.sandbox, &mut self.inspecting);
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

                if ui.button("add node").clicked() {
                    let pos = self.sandbox.cursor;
                    self.sandbox.add_node(pos);
                    self.sync_chat_state();
                }

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

                if ui.button("pair").clicked() {
                    self.pairing_dialog_open = true;
                    self.pairing_a = None;
                    self.pairing_b = None;
                }

                ui.separator();

                let mut range = self.sandbox.medium.borrow().range();
                if ui
                    .add(egui::Slider::new(&mut range, 1.0..=150.0).text("range"))
                    .changed()
                {
                    self.sandbox.medium.borrow_mut().set_range(range);
                }
            });
        });
    }

    fn bottom_bar(&self, ui: &mut Ui) {
        egui::Panel::bottom("bottom_bar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let state = if self.sandbox.running { "run" } else { "pause" };
                ui.label(RichText::new(format!("t={}ms", self.sandbox.clock)).color(theme::META));
                ui.separator();
                ui.label(RichText::new(format!("[{state}]")).color(theme::META));
                ui.separator();
                let hint = "click a node to open its chat; click a wave to inspect; click empty field to move cursor";
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
                    emphasized: self.chats.get(i).is_some_and(|c| c.open),
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
            };
            let output = canvas.show(ui);
            drop(medium);

            let pointer = output.response.interact_pointer_pos();

            if self.dragging.is_none()
                && output.response.drag_started()
                && let Some(point) = pointer
            {
                let pos = screen_to_field(output.inner, point);
                self.dragging = field::nearest_node(&canvas_nodes, pos, field::HIT_THRESHOLD);
            }

            if let Some(idx) = self.dragging {
                if let Some(point) = pointer {
                    self.sandbox
                        .move_node(idx, screen_to_field(output.inner, point));
                }
                if output.response.drag_stopped() {
                    self.dragging = None;
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
