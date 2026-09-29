// src/ui/chat_view.rs

use crate::chat::{ApprovalResolution, ChatBlock, ToolStatus};
use crate::util::formatting::{cap_head, cap_head_lines, cap_tail};
use egui::{CornerRadius, Frame, Margin, Stroke, Ui};

/// Character budget for a model response before it is collapsed with an
/// "Expand" button. Set from a sample of real replies, which ran to several
/// thousand characters; the previous 4,000 truncated most of them.
const RESPONSE_CAP: usize = 12_000;

/// Character budget for tool output, which is usually raw and rarely the thing
/// being read in full.
const TOOL_OUTPUT_CAP: usize = 10_000;

/// Build a configured markdown viewer.
///
/// Syntax highlighting is only available because `egui_commonmark` is pulled
/// in with the `better_syntax_highlighting` feature (see `Cargo.toml`); without
/// it every fenced code block renders as flat proportional text, which is the
/// most visible formatting loss in a coding-agent transcript.
fn markdown_viewer() -> egui_commonmark::CommonMarkViewer<'static> {
    use egui_commonmark::CommonMarkViewer;
    CommonMarkViewer::new()
        .syntax_theme_dark("base16-ocean.dark")
        .syntax_theme_light("base16-ocean.light")
        // Leave the default 4. This is the per-level nesting indent — it is
        // multiplied by (depth - 1) for nested lists and also sets the
        // continuation indent, so 2 pulls every nested bullet and wrapped line
        // too far left. Not worth the tighter look.
        .indentation_spaces(4)
}

/// Render a single ChatBlock into the current UI area.
/// `redirect_texts` stores in-flight redirect-message text per block index.
/// `cache` is the caller-owned markdown cache — it must persist across frames
/// for egui_commonmark's layout reuse to do anything.
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
    cache: &mut egui_commonmark::CommonMarkCache,
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
        // Streaming text goes through the same markdown renderer as the
        // finalized response. Rendering it as a flat label while it streams
        // meant the whole block restyled — reflowed, lost its emphasis and
        // code spans — at the moment the turn ended, which is jarring on long
        // replies and makes the text appear to "jump".
        ChatBlock::ResponseStreaming(text) => {
            markdown_viewer().show(ui, cache, &cap_tail(text, RESPONSE_CAP));
            None
        }
        ChatBlock::Response { text, expanded } => {
            let (display, can_toggle) = if *expanded {
                (text.clone(), text.chars().count() > RESPONSE_CAP)
            } else {
                (cap_head_lines(text, RESPONSE_CAP), false)
            };
            markdown_viewer().show(ui, cache, &display);
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
                    let (display, can_toggle) = if *expanded {
                        (out.clone(), out.chars().count() > TOOL_OUTPUT_CAP)
                    } else {
                        (cap_head_lines(out, TOOL_OUTPUT_CAP), false)
                    };
                    // Tool output is usually raw (JSON, source, logs) rather
                    // than prose, so it stays plain monospace text. Running it
                    // through the markdown renderer would reflow wide JSON into
                    // a hard-to-read ribbon.
                    egui::ScrollArea::horizontal().show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&display)
                                    .monospace()
                                    .color(egui::Color32::from_rgb(0x9a, 0x9a, 0x9a)),
                            )
                            .selectable(true),
                        );
                    });
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

                            // Add the TextEdit BEFORE consuming keys, then gate
                            // on its actual focus. Reading keys first would
                            // steal Enter from the main input box whenever a
                            // pending approval exists further up the transcript.
                            let resp = ui.add_sized(
                                egui::vec2((ui.available_width() - btn_width).max(60.0), 0.0),
                                egui::TextEdit::multiline(text)
                                    .hint_text("redirect with instructions…")
                                    .desired_width(f32::INFINITY)
                                    .min_size(egui::vec2(60.0, 40.0)),
                            );
                            let has_focus = ui.memory(|m| m.focused() == Some(resp.id));

                            // Shift+Enter inserts a newline; Enter submits.
                            if has_focus {
                                if ui.ctx().input_mut(|i| {
                                    i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter)
                                }) {
                                    text.push('\n');
                                }
                                let submit = ui.ctx().input_mut(|i| {
                                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                                });
                                if submit && !text.is_empty() {
                                    let msg = std::mem::take(text);
                                    action = Some(BlockAction::Redirect(index, msg));
                                    resp.request_focus();
                                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Render a block through the real widget path with the real fonts.
    ///
    /// `egui::__run_test_ui` deliberately installs `FontDefinitions::empty()`,
    /// which would let a font-registration regression pass unnoticed. This
    /// uses `Context::run_ui` directly so `fonts::install` actually applies —
    /// without a usable proportional font every label collapses to zero width
    /// and the layout asserts below would pass vacuously.
    fn render(block: &ChatBlock) -> FullOutput {
        let ctx = egui::Context::default();
        crate::ui::fonts::install(&ctx);
        let mut cache = egui_commonmark::CommonMarkCache::default();
        let mut redirects = HashMap::new();
        let mut action = None;
        let mut out = ctx.run_ui(Default::default(), |ui| {
            action = render_block(ui, 0, block, &mut redirects, &mut cache);
        });
        // egui panics on drop if texture/font deltas go unapplied; this test
        // never paints, so discard them the way egui's own harness does.
        out.textures_delta.clear();
        out
    }

    fn shapes(out: &FullOutput) -> usize {
        out.shapes.len()
    }

    #[test]
    fn response_renders_markdown_not_a_flat_label() {
        // A heading, emphasis and a fenced code block. Under the old renderer
        // these were all plain text, so there was no code-block background
        // shape to find.
        let md = "# Title\n\nSome **bold** text.\n\n```rust\nfn main() {}\n```";
        let out = render(&ChatBlock::Response {
            text: md.to_string(),
            expanded: true,
        });
        assert!(
            shapes(&out) > 5,
            "markdown should produce a real layout, got {} shapes",
            shapes(&out)
        );
    }

    #[test]
    fn streaming_response_lays_out_like_a_final_response() {
        // Regression guard: streaming used to render as a flat colored_label,
        // so the text restyled at the moment the turn ended.
        let md = "# Title\n\nSome **bold** text.\n\n```rust\nfn main() {}\n```";
        let streaming = render(&ChatBlock::ResponseStreaming(md.to_string()));
        let final_block = render(&ChatBlock::Response {
            text: md.to_string(),
            expanded: true,
        });
        assert_eq!(
            shapes(&streaming),
            shapes(&final_block),
            "streaming and finalized response should lay out identically"
        );
    }

    #[test]
    fn long_response_collapsed_offers_expand() {
        let long = "line of prose\n".repeat(2000); // well past RESPONSE_CAP
        let out = render(&ChatBlock::Response {
            text: long,
            expanded: false,
        });
        let action = render_action(&ChatBlock::Response {
            text: "line of prose\n".repeat(2000),
            expanded: false,
        });
        assert!(
            action.is_none(),
            "collapsed long response shows the Expand affordance via the button, not an auto action"
        );
        assert!(shapes(&out) > 0);
    }

    #[test]
    fn short_response_has_no_expand_button() {
        // The Expand button is only rendered past the cap, so a short reply
        // should not show it.
        let out = render(&ChatBlock::Response {
            text: "short reply".to_string(),
            expanded: false,
        });
        assert!(shapes(&out) > 0);
    }

    fn render_action(block: &ChatBlock) -> Option<BlockAction> {
        let ctx = egui::Context::default();
        crate::ui::fonts::install(&ctx);
        let mut cache = egui_commonmark::CommonMarkCache::default();
        let mut redirects = HashMap::new();
        let mut action = None;
        let mut out = ctx.run_ui(Default::default(), |ui| {
            action = render_block(ui, 0, block, &mut redirects, &mut cache);
        });
        out.textures_delta.clear();
        action
    }

    use egui::FullOutput;
}
