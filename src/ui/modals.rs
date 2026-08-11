// src/ui/modals.rs

use crate::ui::widgets::{get_models, get_providers, get_sessions};
use crate::util::formatting::{relative_time, StatsContent};
use egui::{CornerRadius, Frame, Margin, Stroke, Ui};

/// Open modal identifier.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ModalId {
    None,
    ModelPicker,
    SessionPicker,
    ProviderInfo,
    Stats,
    Help,
    Context,
    ResumeConfirm(String, u64, u64),
}

/// Render the model picker modal.
pub(crate) fn model_picker(ui: &mut Ui, filter: &mut String, picked: &mut Option<String>) {
    let models = get_models();
    modal_frame(ui, |ui| {
        ui.set_min_width(420.0);
        ui.colored_label(egui::Color32::from_rgb(0xea, 0xea, 0xea), "Select model");
        ui.add(
            egui::TextEdit::singleline(filter)
                .hint_text("filter…")
                .desired_width(f32::INFINITY),
        );
        egui::ScrollArea::vertical()
            .max_height(340.0)
            .show(ui, |ui| {
                let f = filter.to_lowercase();
                let filtered: Vec<_> = if filter.is_empty() {
                    models
                } else {
                    models
                        .into_iter()
                        .filter(|m| {
                            m.id.to_lowercase().contains(&f)
                                || m.provider.to_lowercase().contains(&f)
                        })
                        .collect()
                };
                for entry in &filtered {
                    let label = if entry.is_current {
                        format!("✓ {} · {}", entry.provider, entry.id)
                    } else {
                        format!("{} · {}", entry.provider, entry.id)
                    };
                    if ui.selectable_label(false, &label).clicked() {
                        *picked = Some(entry.id.clone());
                    }
                }
            });
    });
}

/// Render the session picker modal.
pub(crate) fn session_picker(ui: &mut Ui, picked: &mut Option<String>) {
    let sessions = get_sessions();
    modal_frame(ui, |ui| {
        ui.set_min_width(520.0);
        ui.colored_label(egui::Color32::from_rgb(0xea, 0xea, 0xea), "Resume session");
        ui.colored_label(
            egui::Color32::from_rgb(0x7a, 0x7a, 0x7a),
            "Modified   Entries     Session",
        );
        egui::ScrollArea::vertical()
            .max_height(340.0)
            .show(ui, |ui| {
                for entry in &sessions {
                    let row = format!(
                        "{:<11}{:<12}{}",
                        relative_time(entry.mtime_secs),
                        format!("{} entries", entry.entry_count),
                        entry.path
                    );
                    if ui.selectable_label(false, &row).clicked() {
                        *picked = Some(entry.path.clone());
                    }
                }
            });
    });
}

/// Render the provider info modal.
pub(crate) fn provider_info(ui: &mut Ui) {
    let providers = get_providers();
    modal_frame(ui, |ui| {
        ui.set_min_width(460.0);
        ui.colored_label(egui::Color32::from_rgb(0xea, 0xea, 0xea), "Providers");
        egui::ScrollArea::vertical()
            .max_height(340.0)
            .show(ui, |ui| {
                for entry in &providers {
                    let state = if entry.reachable { "✓" } else { "✗" };
                    let flags = match (entry.active, entry.is_external) {
                        (true, true) => " [active, external]",
                        (true, false) => " [active]",
                        (false, true) => " [external]",
                        (false, false) => "",
                    };
                    ui.colored_label(
                        egui::Color32::from_rgb(0xca, 0xca, 0xca),
                        format!("{} {}{}", entry.name, state, flags),
                    );
                }
            });
    });
}

/// Render the body of the stats/context modal as either a two-column table
/// (one grid per section) or plain text.
fn render_stats_content(ui: &mut Ui, content: &StatsContent) {
    match content {
        StatsContent::Sections(sections) => {
            for section in sections {
                ui.add_space(6.0);
                ui.colored_label(
                    egui::Color32::from_rgb(0xea, 0xea, 0xea),
                    &section.title,
                );
                ui.add_space(2.0);
                egui::Grid::new(format!("stats_grid_{}", section.title))
                    .num_columns(2)
                    .spacing(egui::vec2(12.0, 4.0))
                    .min_col_width(120.0)
                    .show(ui, |ui| {
                        for row in &section.rows {
                            ui.colored_label(
                                egui::Color32::from_rgb(0xca, 0xca, 0xca),
                                &row.label,
                            );
                            ui.colored_label(egui::Color32::from_rgb(0xca, 0xca, 0xca), &row.value);
                            ui.end_row();
                        }
                    });
            }
        }
        StatsContent::Text(body) => {
            ui.colored_label(egui::Color32::from_rgb(0xca, 0xca, 0xca), body);
        }
    }
}

/// Render the stats modal.
pub(crate) fn stats_modal(ui: &mut Ui, content: &StatsContent) {
    modal_frame(ui, |ui| {
        ui.set_min_width(500.0);
        ui.colored_label(egui::Color32::from_rgb(0xea, 0xea, 0xea), "Session stats");
        egui::ScrollArea::vertical()
            .max_height(450.0)
            .show(ui, |ui| {
                render_stats_content(ui, content);
            });
    });
}

/// Render the help modal.
pub(crate) fn help_modal(ui: &mut Ui) {
    modal_frame(ui, |ui| {
        ui.set_min_width(680.0);
        ui.colored_label(
            egui::Color32::from_rgb(0xea, 0xea, 0xea),
            "rho — quick reference",
        );
        egui::ScrollArea::vertical()
            .max_height(450.0)
            .show(ui, |ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(0xca, 0xca, 0xca),
                    "Type a message and press Enter to chat with the agent.\n\n\
                     Menu buttons:\n  \
                     Session — list and resume previous sessions\n  \
                     Resume Last — quickly resume the most recent session\n  \
                     Model — pick a model from the scrollable list\n  \
                     Providers — view configured providers and their status\n  \
                     Extensions — list installed extensions\n  \
                     Reload — reload extensions from disk\n  \
                     Context — inspect context usage; Compact, Clear, or start a New Session\n  \
                     Restart — kill and re-spawn the rho subprocess\n  \
                     Abort — cancel the current agent turn\n  \
                     Stats — view session stats (usage, tokens, cost)\n  \
                     Help — this dialog\n  \
                     Quit — exit rho\n\
                     Input: Enter sends, Shift+Enter inserts a newline.\n\n\
                     While the agent is working, your message is sent as a mid-turn\n\
                     steering prompt instead of starting a new turn.",
                );
            });
    });
}

/// Render the resume-last-session confirmation modal.
pub(crate) fn resume_confirm(ui: &mut Ui, path: &str, mtime_secs: u64, entry_count: u64) {
    modal_frame(ui, |ui| {
        ui.set_min_width(460.0);
        ui.colored_label(
            egui::Color32::from_rgb(0xea, 0xea, 0xea),
            "Resume last session?",
        );
        ui.colored_label(
            egui::Color32::from_rgb(0x9a, 0x9a, 0x9a),
            format!("{} · {} entries", relative_time(mtime_secs), entry_count),
        );
        ui.colored_label(egui::Color32::from_rgb(0x7a, 0x7a, 0x7a), path);
    });
}

/// Action returned from the context management modal.
#[derive(Clone, Debug)]
pub(crate) enum ContextAction {
    Compact,
    Clear,
    NewSession,
}

/// Render the context management modal.
/// Returns `Some(action)` if an action button was clicked (modal should close).
/// Returns `None` if no action was taken this frame.
pub(crate) fn context_modal(ui: &mut Ui, content: &StatsContent) -> Option<ContextAction> {
    let mut action = None;
    modal_frame(ui, |ui| {
        ui.set_min_width(500.0);
        ui.colored_label(egui::Color32::from_rgb(0xea, 0xea, 0xea), "Context management");
        egui::ScrollArea::vertical()
            .max_height(450.0)
            .show(ui, |ui| {
                render_stats_content(ui, content);
            });
        ui.horizontal(|ui| {
            if ui.button("Compact").clicked() {
                action = Some(ContextAction::Compact);
            }
            if ui.button("Clear").clicked() {
                action = Some(ContextAction::Clear);
            }
            if ui.button("New Session").clicked() {
                action = Some(ContextAction::NewSession);
            }
        });
    });
    action
}

// ── Shared modal frame

fn modal_frame(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    let frame = Frame {
        fill: egui::Color32::from_rgb(0x1b, 0x1b, 0x20),
        inner_margin: Margin::symmetric(10, 10),
        corner_radius: CornerRadius::same(4),
        stroke: Stroke::new(1.0, egui::Color32::from_rgb(0x3a, 0x3a, 0x40)),
        ..Default::default()
    };
    frame.show(ui, content);
}
