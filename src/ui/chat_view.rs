// src/ui/chat_view.rs

use crate::chat::{ApprovalResolution, ChatBlock, ToolStatus};
use crate::util::formatting::{cap_head, cap_tail};
use egui::{CornerRadius, Frame, Margin, Stroke, Ui};

/// Render a single ChatBlock into the current UI area.
/// `redirect_texts` stores in-flight redirect-message text per block index.
/// Returns `Some(action)` if the user triggered an inline action.
#[derive(Clone, Debug)]
pub(crate) enum BlockAction {
    ExpandToggle(usize),
    Approve(usize),
    Deny(usize),
    Redirect(usize, String),
}

pub(crate) fn render_block(
    ui: &mut Ui,
    index: usize,
    block: &ChatBlock,
    redirect_texts: &mut std::collections::HashMap<usize, String>,
) -> Option<BlockAction> {
    match block {
        ChatBlock::User(text) => render_frame(ui, "#303045", |ui| {
            ui.colored_label(egui::Color32::from_rgb(0xdc, 0xdc, 0xdc), text);
            None
        }),
        ChatBlock::Steer(text) => render_frame(ui, "#3a2a00", |ui| {
            ui.colored_label(egui::Color32::from_rgb(0xea, 0xea, 0xea), text);
            None
        }),
        ChatBlock::Reasoning { text, elapsed_secs } => {
            ui.colored_label(
                egui::Color32::from_rgb(0x7a, 0x7a, 0x7a),
                format!("? thought · {elapsed_secs}"),
            );
            ui.colored_label(
                egui::Color32::from_rgb(0x7a, 0x7a, 0x7a),
                cap_head(text, 4000),
            );
            None
        }
        ChatBlock::ReasoningStreaming { text } => {
            ui.colored_label(egui::Color32::from_rgb(0x7a, 0x7a, 0x7a), "? thinking");
            ui.colored_label(
                egui::Color32::from_rgb(0x7a, 0x7a, 0x7a),
                cap_tail(text, 4000),
            );
            None
        }
        ChatBlock::ResponseStreaming(text) => {
            ui.colored_label(
                egui::Color32::from_rgb(0xdc, 0xdc, 0xdc),
                cap_tail(text, 4000),
            );
            None
        }
        ChatBlock::Response { text, expanded } => {
            use egui_commonmark::CommonMarkViewer;
            let display = if *expanded {
                text.clone()
            } else {
                cap_head(text, 4000)
            };
            let mut cache = egui_commonmark::CommonMarkCache::default();
            CommonMarkViewer::new().show(ui, &mut cache, &display);
            let can_toggle = text.chars().count() > 4000;
            if can_toggle {
                let label = if *expanded { "Collapse" } else { "Expand" };
                if ui.small_button(label).clicked() {
                    return Some(BlockAction::ExpandToggle(index));
                }
            }
            None
        }
        ChatBlock::ToolCall {
            name,
            args,
            status,
            output,
            expanded,
        } => {
            let (bg, status_text) = match status {
                ToolStatus::Success => ("#00551a", "✓ done"),
                ToolStatus::Pending => ("#2e2e2e", "⟳ running"),
                ToolStatus::Error => ("#5f1a1a", "✗ failed"),
                ToolStatus::Denied => ("#2e2e2e", "⊘ denied"),
            };
            render_frame(ui, bg, |ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(0xea, 0xea, 0xea),
                    format!("{name} {args} {status_text}"),
                );
                if let Some(out) = output {
                    let display = if *expanded {
                        out.clone()
                    } else {
                        cap_head(out, 10000)
                    };
                    ui.colored_label(egui::Color32::from_rgb(0x9a, 0x9a, 0x9a), display);
                    let can_toggle = out.chars().count() > 10000;
                    if can_toggle {
                        let label = if *expanded { "Collapse" } else { "Expand" };
                        if ui.small_button(label).clicked() {
                            return Some(BlockAction::ExpandToggle(index));
                        }
                    }
                }
                None
            })
        }
        ChatBlock::Info(text) => {
            ui.colored_label(egui::Color32::from_rgb(0x6a, 0x6a, 0x6a), text);
            None
        }
        ChatBlock::Approval {
            tool,
            arguments,
            risk,
            resolution,
        } => {
            render_frame(ui, "#3a3000", |ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(0xea, 0xea, 0xea),
                    format!("{tool} {arguments} ({risk})"),
                );
                match resolution {
                    ApprovalResolution::Pending => {
                        let mut action: Option<BlockAction> = None;
                        ui.horizontal(|ui| {
                            if ui.button("Approve").clicked() {
                                action = Some(BlockAction::Approve(index));
                            }
                            if ui.button("Deny").clicked() {
                                action = Some(BlockAction::Deny(index));
                            }
                        });
                        ui.horizontal(|ui| {
                            let text = redirect_texts.entry(index).or_default();
                            let btn_width = 80.0;

                            // Consume Enter (without Shift) to submit,
                            // Shift+Enter to insert newline.
                            let submit = ui.ctx().input_mut(|i| {
                                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                            });
                            if ui.ctx().input_mut(|i| {
                                i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter)
                            }) {
                                text.push('\n');
                            }

                            let resp = ui.add_sized(
                                egui::vec2((ui.available_width() - btn_width).max(60.0), 0.0),
                                egui::TextEdit::multiline(text)
                                    .hint_text("redirect with instructions…")
                                    .desired_width(f32::INFINITY)
                                    .min_size(egui::vec2(60.0, 40.0)),
                            );
                            if submit && !text.is_empty() {
                                let msg = std::mem::take(text);
                                action = Some(BlockAction::Redirect(index, msg));
                                resp.request_focus();
                            }
                            if ui.button("Redirect").clicked() && !text.is_empty() {
                                let msg = std::mem::take(text);
                                action = Some(BlockAction::Redirect(index, msg));
                                resp.request_focus();
                            }
                        });
                        action
                    }
                    ApprovalResolution::Approved => {
                        ui.colored_label(egui::Color32::from_rgb(0x9a, 0x9a, 0x9a), "✓ approved");
                        None
                    }
                    ApprovalResolution::Denied => {
                        ui.colored_label(egui::Color32::from_rgb(0x9a, 0x9a, 0x9a), "✗ denied");
                        None
                    }
                    ApprovalResolution::Redirected(msg) => {
                        ui.colored_label(
                            egui::Color32::from_rgb(0x9a, 0x9a, 0x9a),
                            format!("↪ redirected: {msg}"),
                        );
                        None
                    }
                }
            })
        }
    }
}

// ── Frame helpers ─────────────────────────────────────────────────────────

/// Render a filled rect block (like Makepad's SolidView).
fn render_frame(
    ui: &mut Ui,
    hex_bg: &str,
    content: impl FnOnce(&mut Ui) -> Option<BlockAction>,
) -> Option<BlockAction> {
    let bg = parse_hex(hex_bg);
    let frame = Frame {
        fill: bg,
        inner_margin: Margin::symmetric(8, 8),
        corner_radius: CornerRadius::same(0),
        stroke: Stroke::NONE,
        ..Default::default()
    };
    frame
        .show(ui, |ui| {
            ui.set_min_height(0.0);
            content(ui)
        })
        .inner
}

fn parse_hex(hex: &str) -> egui::Color32 {
    let hex = hex.trim_start_matches("#x").trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    egui::Color32::from_rgb(r, g, b)
}
