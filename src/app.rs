// src/app.rs

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::agent::{RhoAgent, RhoEvent};
use crate::chat::{ApprovalResolution, ChatBlock, ToolStatus, store as chat_store};
use crate::ui::chat_view::{BlockAction, render_block};
use crate::ui::modals::{ContextAction, ModalId};
use crate::ui::widgets;
use crate::util::formatting::{
    format_secs, format_session_stats, format_usage_stats, StatsContent,
};
use crate::util::json::{jbool, jf64, jstr, ju64};

pub struct App {
    agent: Option<RhoAgent>,
    input_text: String,

    // Working / busy
    busy: bool,
    working_start: Option<Instant>,
    working_state: String,
    steer_count: u32,

    // Footer state
    usage: UsageState,
    current_model: String,
    current_cwd: String,

    // Model list
    models: Vec<(String, String)>,
    model_filter_text: String,

    // Stats cache
    stats_body: StatsContent,

    // Modal control
    open_modal: ModalId,

    // "Resume Last" flag — next ListSessions response opens ResumeConfirm
    resume_latest_requested: bool,

    // Context modal flag — next GetSessionStats response opens Context modal
    context_modal_requested: bool,

    // Context modal body cache
    context_body: StatsContent,

    // Redirect input text per approval block index
    redirect_texts: HashMap<usize, String>,

    // Have we spawned the agent yet?
    agent_spawned: bool,
}

#[derive(Default)]
struct UsageState {
    input: u64,
    output: u64,
    cached: u64,
    cost: f64,
    ctx_used: u64,
    ctx_window: u64,
    util: u8,
}

impl Default for App {
    fn default() -> Self {
        Self {
            agent: None,
            input_text: String::new(),
            busy: false,
            working_start: None,
            working_state: String::new(),
            steer_count: 0,
            usage: UsageState::default(),
            current_model: String::new(),
            current_cwd: String::new(),
            models: Vec::new(),
            model_filter_text: String::new(),
            stats_body: StatsContent::text(String::new()),
            open_modal: ModalId::None,
            resume_latest_requested: false,
            context_modal_requested: false,
            context_body: StatsContent::text(String::new()),
            redirect_texts: HashMap::new(),
            agent_spawned: false,
        }
    }
}

impl App {
    fn start_working(&mut self) {
        self.busy = true;
        self.working_start = Some(Instant::now());
        self.working_state.clear();
    }

    fn stop_working(&mut self) {
        self.busy = false;
        self.working_start = None;
        self.steer_count = 0;
    }

    fn push_block(&self, block: ChatBlock) {
        chat_store::push_after_finalizing_response(block);
    }

    fn format_usage(&self) -> String {
        let u = &self.usage;
        let k = |n: u64| {
            if n == 0 {
                "0".to_string()
            } else {
                format!("{:.1}k", n as f64 / 1000.0)
            }
        };
        if u.ctx_window == 0 && u.ctx_used == 0 {
            format!(
                "in {}    out {}    R {}    ${:.3}",
                k(u.input),
                k(u.output),
                k(u.cached),
                u.cost
            )
        } else {
            format!(
                "in {}    out {}    R {}    ${:.3}    ctx {}/{} ({}%)",
                k(u.input),
                k(u.output),
                k(u.cached),
                u.cost,
                k(u.ctx_used),
                k(u.ctx_window),
                u.util,
            )
        }
    }

    fn spawn_agent(&mut self, ctx: egui::Context) {
        self.agent_spawned = true;
        self.push_block(ChatBlock::Info("Starting rho…".into()));
        let repaint = Arc::new(move || ctx.request_repaint());
        match RhoAgent::spawn(repaint) {
            Ok(agent) => self.agent = Some(agent),
            Err(e) => {
                self.push_block(ChatBlock::Info(format!(
                    "⚠ Could not start rho: {e}\nSet RHO_PATH to the rho binary, or put rho on PATH.",
                )));
            }
        }
    }

    fn restart_agent(&mut self, ctx: egui::Context) {
        self.agent = None;
        self.stop_working();
        self.push_block(ChatBlock::Info("Restarting rho…".into()));
        let repaint = Arc::new(move || ctx.request_repaint());
        match RhoAgent::spawn(repaint) {
            Ok(agent) => self.agent = Some(agent),
            Err(e) => {
                self.push_block(ChatBlock::Info(format!("⚠ Could not restart rho: {e}")));
            }
        }
    }

    // ── Event handling ────────────────────────────────────────────────────

    fn handle_rho_event(&mut self, ev: RhoEvent) {
        match ev {
            RhoEvent::Ready => {
                self.push_block(ChatBlock::Info("✓ Connected to rho".into()));
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_state();
                    let _ = agent.list_models();
                    let _ = agent.get_session_stats();
                }
            }
            RhoEvent::AgentStart => self.start_working(),
            RhoEvent::MessageDelta { delta } => chat_store::append_streaming_response(&delta),
            RhoEvent::ReasoningDelta { delta } => {
                chat_store::finalize_streaming_response();
                chat_store::append_streaming_reasoning(&delta);
            }
            RhoEvent::AgentEnd { reply, duration_ms } => {
                chat_store::finalize_reasoning(format_secs(duration_ms));
                chat_store::finalize_streaming_response();
                chat_store::push_final_response_if_missing(reply);
                self.stop_working();
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_session_stats();
                }
            }
            RhoEvent::AgentError { error } => {
                self.push_block(ChatBlock::Info(format!("⚠ {error}")));
                self.stop_working();
            }
            RhoEvent::StateChange { state } => {
                self.working_state = state.clone();
                if state == "idle" && self.busy {
                    self.stop_working();
                }
            }
            RhoEvent::ToolCall { name, arguments } => {
                chat_store::finalize_streaming_response();
                chat_store::push_pending_tool_call(name, arguments);
            }
            RhoEvent::ToolResult {
                name,
                is_error,
                output,
            } => {
                let status = if is_error {
                    ToolStatus::Error
                } else {
                    ToolStatus::Success
                };
                let output = if output.is_empty() {
                    None
                } else {
                    Some(output)
                };
                chat_store::finalize_tool_call(&name, status, output);
            }
            RhoEvent::ToolDenied { name } => {
                chat_store::finalize_tool_call(
                    &name,
                    ToolStatus::Denied,
                    Some("denied by approval gate".into()),
                );
            }
            RhoEvent::ApprovalRequest {
                tool,
                arguments,
                risk,
            } => {
                chat_store::push_approval(tool, arguments, risk);
            }
            RhoEvent::Usage {
                input_tokens,
                output_tokens,
                cached_tokens,
                cost,
                context_used,
                context_window,
                utilization,
            } => {
                self.usage.input += input_tokens;
                self.usage.output += output_tokens;
                self.usage.cached += cached_tokens;
                self.usage.cost += cost;
                self.usage.ctx_used = context_used;
                self.usage.ctx_window = context_window;
                self.usage.util = utilization;
            }
            RhoEvent::Response { kind, result } => self.handle_response(kind, result),
            RhoEvent::RequestError { kind, error } => {
                self.push_block(ChatBlock::Info(format!("⚠ {kind:?} failed: {error}")));
            }
            RhoEvent::Closed => {
                self.push_block(ChatBlock::Info("rho process exited.".into()));
                self.agent = None;
                self.stop_working();
            }
        }
    }

    fn handle_response(&mut self, kind: crate::agent::RequestKind, result: serde_json::Value) {
        use crate::agent::RequestKind;
        match kind {
            RequestKind::GetState => {
                self.current_model = jstr(Some(&result), "model");
                self.current_cwd = jstr(Some(&result), "cwd");
            }
            RequestKind::ListModels => {
                let mut models: Vec<(String, String)> = result
                    .get("models")
                    .and_then(|m| m.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|e| {
                                let id = e.get("id")?.as_str()?.to_owned();
                                Some((id, jstr(Some(e), "provider")))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                models.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
                self.models = models;
                self.sync_model_list();
            }
            RequestKind::SetModel => {
                let model = jstr(Some(&result), "model");
                self.current_model = model.clone();
                self.push_block(ChatBlock::Info(format!("→ model: {model}")));
                if result.get("contextWindow").is_some() {
                    self.usage.ctx_window = ju64(Some(&result), "contextWindow");
                    self.usage.ctx_used = ju64(Some(&result), "estimatedUsed");
                    self.usage.util = ju64(Some(&result), "utilizationPercent").min(255) as u8;
                }
            }
            RequestKind::ListProviders => {
                let mut providers = Vec::new();
                if let Some(arr) = result.get("providers").and_then(|x| x.as_array()) {
                    for p in arr {
                        providers.push(widgets::ProviderEntry {
                            name: jstr(Some(p), "name"),
                            reachable: jbool(Some(p), "reachable"),
                            active: jbool(Some(p), "active"),
                            is_external: jbool(Some(p), "isExternal"),
                        });
                    }
                }
                if providers.is_empty() {
                    self.push_block(ChatBlock::Info("No providers configured.".into()));
                } else {
                    widgets::set_providers(providers);
                    self.open_modal = ModalId::ProviderInfo;
                }
            }
            RequestKind::ListSessions => {
                let mut sessions: Vec<widgets::SessionEntry> = result
                    .get("sessions")
                    .and_then(|x| x.as_array())
                    .map(|arr| {
                        arr.iter()
                            .map(|s| widgets::SessionEntry {
                                path: jstr(Some(s), "path"),
                                mtime_secs: ju64(Some(s), "mtimeSecs"),
                                entry_count: ju64(Some(s), "entryCount"),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                sessions.sort_by_key(|b| std::cmp::Reverse(b.mtime_secs));
                if sessions.is_empty() {
                    self.push_block(ChatBlock::Info("No previous sessions.".into()));
                } else if self.resume_latest_requested {
                    self.resume_latest_requested = false;
                    let newest = sessions[0].clone();
                    self.open_modal =
                        ModalId::ResumeConfirm(newest.path, newest.mtime_secs, newest.entry_count);
                } else {
                    widgets::set_sessions(sessions.clone());
                    self.open_modal = ModalId::SessionPicker;
                }
            }
            RequestKind::ResumeSession => {
                self.current_model = jstr(Some(&result), "model");
                self.current_cwd = jstr(Some(&result), "cwd");
                self.push_block(ChatBlock::Info(format!(
                    "↻ resumed session ({})",
                    self.current_cwd
                )));
            }
            RequestKind::GetSessionStats => {
                let api = result.get("apiUsage");
                self.usage.input = ju64(api, "totalInputTokens");
                self.usage.output = ju64(api, "totalOutputTokens");
                self.usage.cached = ju64(api, "totalCachedTokens");
                self.usage.cost = jf64(api, "totalCost");
                self.usage.ctx_used = ju64(Some(&result), "estimatedUsed");
                self.usage.ctx_window = ju64(Some(&result), "contextWindow");
                self.usage.util = ju64(Some(&result), "utilizationPercent").min(255) as u8;
                self.stats_body = StatsContent::sections(format_session_stats(&result));
                // Context modal: same response feeds this modal
                // when the user opened it via the Context button.
                if self.context_modal_requested {
                    self.context_modal_requested = false;
                    self.context_body = self.stats_body.clone();
                    self.open_modal = ModalId::Context;
                }
            }
            RequestKind::ReloadExtensions => {
                let reloaded = ju64(Some(&result), "reloaded");
                let added = ju64(Some(&result), "added");
                let removed = ju64(Some(&result), "removed");
                self.push_block(ChatBlock::Info(format!(
                    "✓ extensions reloaded (+{added} ~{reloaded} -{removed})"
                )));
            }
            RequestKind::ListExtensions => {
                let body = if let Some(arr) = result.get("extensions").and_then(|x| x.as_array()) {
                    if arr.is_empty() {
                        "No extensions installed.".to_string()
                    } else {
                        arr.iter()
                            .map(|e| {
                                let name = jstr(Some(e), "name");
                                let status = jstr(Some(e), "status");
                                let tools = ju64(Some(e), "toolCount");
                                if status.is_empty() {
                                    if tools > 0 {
                                        format!("  {name} ({tools} tools)")
                                    } else {
                                        format!("  {name}")
                                    }
                                } else if tools > 0 {
                                    format!("  {name} — {status} ({tools} tools)")
                                } else {
                                    format!("  {name} — {status}")
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                } else {
                    serde_json::to_string_pretty(&result).unwrap_or_default()
                };
                self.stats_body = StatsContent::text(body);
                self.open_modal = ModalId::Stats;
            }
            RequestKind::Compact => {
                self.push_block(ChatBlock::Info("✓ compacted".into()));
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_session_stats();
                }
            }
            RequestKind::Clear => {
                chat_store::clear();
                self.push_block(ChatBlock::Info("✓ conversation cleared".into()));
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_session_stats();
                }
            }
            RequestKind::NewSession => {
                chat_store::clear();
                self.push_block(ChatBlock::Info("✓ new session".into()));
                if let Some(agent) = &mut self.agent {
                    let _ = agent.get_session_stats();
                }
            }
        }
    }

    fn sync_model_list(&mut self) {
        let current = self.current_model.as_str();
        let models: Vec<widgets::ModelEntry> = self
            .models
            .iter()
            .filter(|(id, _provider)| {
                let f = self.model_filter_text.to_lowercase();
                f.is_empty()
                    || id.to_lowercase().contains(&f)
                    || _provider.to_lowercase().contains(&f)
            })
            .map(|(id, provider)| widgets::ModelEntry {
                id: id.clone(),
                provider: provider.clone(),
                is_current: id.as_str() == current,
            })
            .collect();
        widgets::set_models(models);
    }

    fn submit_message(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let steer = self.busy;
        chat_store::push(if steer {
            ChatBlock::Steer(text.to_string())
        } else {
            ChatBlock::User(text.to_string())
        });
        let result = if let Some(agent) = &mut self.agent {
            if steer {
                self.steer_count += 1;
                self.working_state = "steered".into();
                agent.prompt(text, true)
            } else {
                agent.prompt(text, false)
            }
        } else {
            Err("rho agent not connected.".into())
        };
        if let Err(e) = result {
            chat_store::push(ChatBlock::Info(format!("⚠ {e}")));
        }
    }

    fn resolve_approval(&mut self, index: usize, approved: bool, message: Option<String>) {
        let msg = message.filter(|m| !m.is_empty());
        let resolution = match (approved, &msg) {
            (true, _) => ApprovalResolution::Approved,
            (false, Some(m)) => ApprovalResolution::Redirected(m.clone()),
            (false, None) => ApprovalResolution::Denied,
        };
        chat_store::resolve_approval(index, resolution);
        if let Some(agent) = &mut self.agent {
            let _ = agent.approval_response(approved, msg);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // ── 0. Spawn agent on first frame ────────────────────────────────
        if !self.agent_spawned {
            self.spawn_agent(ctx.clone());
        }

        // ── 1. Drain agent events ─────────────────────────────────────────
        let events = self.agent.as_mut().map(|a| a.drain()).unwrap_or_default();
        for ev in events {
            self.handle_rho_event(ev);
        }

        // ── 2. Style tweaks ───────────────────────────────────────────────
        {
            let theme = egui::Theme::from_dark_mode(true);
            let mut style = ctx.style_of(theme).as_ref().clone();
            style.spacing.item_spacing.y = 0.0;
            style.spacing.button_padding = egui::vec2(6.0, 2.0);
            style.visuals.widgets.noninteractive.bg_fill =
                egui::Color32::from_rgb(0x0f, 0x0f, 0x12);
            style.visuals.panel_fill = egui::Color32::from_rgb(0x0f, 0x0f, 0x12);
            ctx.set_style_of(theme, Arc::new(style));
        }

        // ── 3. Title bar ──────────────────────────────────────────────────
        egui::Panel::top("title_bar")
            .frame(egui::Frame {
                fill: egui::Color32::from_rgb(0x1b, 0x1b, 0x20),
                ..Default::default()
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("rho")
                            .color(egui::Color32::from_rgb(0xea, 0xea, 0xea))
                            .size(18.0)
                            .strong(),
                    );
                    ui.colored_label(
                        egui::Color32::from_rgb(0x6a, 0x6a, 0x6a),
                        "Rust programming assistant",
                    );
                });
            });

        // ── 4. Menu bar ───────────────────────────────────────────────────
        egui::Panel::top("menu_bar")
            .frame(egui::Frame {
                fill: egui::Color32::from_rgb(0x15, 0x15, 0x1a),
                ..Default::default()
            })
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let btn =
                        |ui: &mut egui::Ui, label: &str| -> bool { ui.button(label).clicked() };
                    if btn(ui, "Session") {
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.list_sessions();
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Resume Last") {
                        self.resume_latest_requested = true;
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.list_sessions();
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Model") {
                        self.model_filter_text.clear();
                        self.sync_model_list();
                        self.open_modal = ModalId::ModelPicker;
                    }
                    if btn(ui, "Providers") {
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.list_providers();
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Extensions") {
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.list_extensions();
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Reload") {
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.reload_extensions();
                            self.push_block(ChatBlock::Info("Reloading extensions…".into()));
                        } else {
                            self.push_block(ChatBlock::Info("not connected.".into()));
                        }
                    }
                    if btn(ui, "Context") {
                        if self.busy {
                            // Render from cached usage data so the modal opens instantly
                            let u = &self.usage;
                            self.context_body = StatsContent::sections(format_usage_stats(
                                u.input, u.output, u.cached, u.cost,
                                u.ctx_used, u.ctx_window, u.util,
                            ));
                            self.open_modal = ModalId::Context;
                        } else {
                            match &mut self.agent {
                                Some(agent) => {
                                    self.context_modal_requested = true;
                                    let _ = agent.get_session_stats();
                                }
                                None => self.push_block(ChatBlock::Info("not connected.".into())),
                            }
                        }
                    }
                    if btn(ui, "Restart") {
                        self.restart_agent(ctx.clone());
                    }
                    if btn(ui, "Abort") {
                        if !self.busy {
                            self.push_block(ChatBlock::Info("nothing to abort.".into()));
                        } else if let Some(agent) = &mut self.agent {
                            let _ = agent.abort();
                            self.working_state = "aborting".into();
                        }
                    }
                    if btn(ui, "Stats") {
                        if let Some(agent) = &mut self.agent {
                            let _ = agent.get_session_stats();
                        }
                        self.open_modal = ModalId::Stats;
                    }
                    if btn(ui, "Help") {
                        self.open_modal = ModalId::Help;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if btn(ui, "Quit") {
                            std::process::exit(0);
                        }
                    });
                });
            });

        // ── 5. Bottom panel: Working line + Input + Footer ────────────────
        egui::Panel::bottom("bottom_area")
            .frame(egui::Frame {
                fill: egui::Color32::from_rgb(0x0f, 0x0f, 0x12),
                ..Default::default()
            })
            .min_size(90.0)
            .show(ui, |ui| {
                // Working line
                if self.busy {
                    let elapsed = self.working_start.map(|s| s.elapsed()).unwrap_or_default();
                    const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
                    let ch = SPINNER[((elapsed.as_millis() / 80) as usize) % SPINNER.len()];
                    let state = if self.working_state.is_empty() {
                        "thinking".to_string()
                    } else {
                        self.working_state.clone()
                    };
                    let badge = if self.steer_count > 0 {
                        format!("  ↗ {} steered", self.steer_count)
                    } else {
                        String::new()
                    };
                    ui.colored_label(
                        egui::Color32::from_rgb(0xaa, 0xaa, 0x00),
                        format!(
                            "{ch} Working  {:.1}s  {state}{badge}",
                            elapsed.as_secs_f64()
                        ),
                    );
                    ctx.request_repaint_after(Duration::from_millis(80));
                }

                // Input box
                egui::Frame {
                    fill: egui::Color32::TRANSPARENT,
                    stroke: egui::Stroke::new(1.0, egui::Color32::from_rgb(0x4a, 0x4a, 0x4a)),
                    inner_margin: egui::Margin::symmetric(10, 8),
                    ..Default::default()
                }
                .show(ui, |ui| {
                    let placeholder = if self.busy {
                        "Steer the agent... (Enter to send, Shift+Enter for newline)"
                    } else {
                        "Type a message... (Enter to send, Shift+Enter for newline)"
                    };

                    let mut submit = false;

                    let resp = ui.add_sized(
                        egui::vec2(ui.available_width(), 50.0),
                        egui::TextEdit::multiline(&mut self.input_text)
                            .hint_text(placeholder)
                            .desired_width(f32::INFINITY)
                            .lock_focus(true)
                            .min_size(egui::vec2(ui.available_width(), 50.0)),
                    );

                    // Only consume keyboard events for the main input if it has focus,
                    // so the redirect text box can also use Enter/Shift+Enter.
                    let main_has_focus =
                        ui.memory(|m| m.focused() == Some(resp.id));
                    if main_has_focus {
                        if ctx
                            .input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Enter))
                        {
                            self.input_text.push('\n');
                        }
                        submit = ctx
                            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                    }
                    if submit {
                        let text = std::mem::take(&mut self.input_text);
                        self.submit_message(&text);
                        resp.request_focus();
                    }
                });

                // Footer
                egui::Frame {
                    fill: egui::Color32::from_rgb(0x1b, 0x1b, 0x20),
                    inner_margin: egui::Margin::symmetric(12, 6),
                    ..Default::default()
                }
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(0x6a, 0x6a, 0x6a),
                            &self.current_cwd,
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.colored_label(
                            egui::Color32::from_rgb(0x6a, 0x6a, 0x6a),
                            self.format_usage(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.colored_label(
                                egui::Color32::from_rgb(0x6a, 0x6a, 0x6a),
                                &self.current_model,
                            );
                        });
                    });
                });
            });

        // ── 6. Central panel: Chat scrollback ─────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame {
                fill: egui::Color32::from_rgb(0x0f, 0x0f, 0x12),
                inner_margin: egui::Margin::symmetric(12, 8),
                ..Default::default()
            })
            .show(ui, |ui| {
                let blocks = chat_store::snapshot();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        for (i, block) in blocks.iter().enumerate() {
                            if let Some(action) =
                                render_block(ui, i, block, &mut self.redirect_texts)
                            {
                                match action {
                                    BlockAction::ExpandToggle(idx) => {
                                        chat_store::toggle_expand(idx)
                                    }
                                    BlockAction::Approve(idx) => {
                                        self.resolve_approval(idx, true, None)
                                    }
                                    BlockAction::Deny(idx) => {
                                        self.resolve_approval(idx, false, None)
                                    }
                                    BlockAction::Redirect(idx, msg) => {
                                        self.resolve_approval(idx, false, Some(msg))
                                    }
                                }
                            }
                        }
                    });
            });

        // ── 7. Modals ─────────────────────────────────────────────────────
        let open = self.open_modal.clone();
        if open != ModalId::None {
            let mut close_modal = false;
            let mut picked_model: Option<String> = None;
            let mut picked_session: Option<String> = None;

            let title = match &open {
                ModalId::ModelPicker => "Select model",
                ModalId::SessionPicker => "Resume session",
                ModalId::ProviderInfo => "Providers",
                ModalId::Stats => "Session stats",
                ModalId::Help => "Help",
                ModalId::Context => "Context management",
                ModalId::ResumeConfirm(..) => "Resume last session?",
                ModalId::None => unreachable!(),
            };
            egui::Window::new(title)
                .resizable(false)
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(egui::Frame {
                    fill: egui::Color32::from_rgb(0x1b, 0x1b, 0x20),
                    ..Default::default()
                })
                .show(&ctx, |ui| {
                    match &open {
                        ModalId::ModelPicker => {
                            crate::ui::modals::model_picker(
                                ui,
                                &mut self.model_filter_text,
                                &mut picked_model,
                            );
                        }
                        ModalId::SessionPicker => {
                            crate::ui::modals::session_picker(ui, &mut picked_session);
                        }
                        ModalId::ProviderInfo => crate::ui::modals::provider_info(ui),
                        ModalId::Stats => crate::ui::modals::stats_modal(ui, &self.stats_body),
                        ModalId::Help => crate::ui::modals::help_modal(ui),
                        ModalId::ResumeConfirm(path, mtime, count) => {
                            crate::ui::modals::resume_confirm(ui, path, *mtime, *count);
                        }
                        ModalId::None => unreachable!(),
                        ModalId::Context => {} // handled below
                    }
                    // Context modal: action buttons return a ContextAction
                    if let ModalId::Context = &open {
                        if let Some(action) =
                            crate::ui::modals::context_modal(ui, &self.context_body)
                        {
                            close_modal = true;
                            if let Some(agent) = &mut self.agent {
                                match action {
                                    ContextAction::Compact => { let _ = agent.compact(); }
                                    ContextAction::Clear => { let _ = agent.clear(); }
                                    ContextAction::NewSession => { let _ = agent.new_session(); }
                                }
                            }
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            match &open {
                                ModalId::ResumeConfirm(path, ..) => {
                                    if ui.button("Cancel").clicked() {
                                        close_modal = true;
                                    }
                                    if ui.button("Resume").clicked() {
                                        if let Some(agent) = &mut self.agent {
                                            let _ = agent.resume_session(path);
                                        }
                                        close_modal = true;
                                    }
                                }
                                _ => {
                                    if ui.button("Close").clicked() {
                                        close_modal = true;
                                    }
                                }
                            }
                        });
                    });
                });

            if close_modal {
                self.open_modal = ModalId::None;
            }
            if let Some(model) = picked_model {
                if let Some(agent) = &mut self.agent {
                    let _ = agent.set_model(&model);
                }
                self.open_modal = ModalId::None;
            }
            if let Some(path) = picked_session {
                if let Some(agent) = &mut self.agent {
                    let _ = agent.resume_session(&path);
                }
                self.open_modal = ModalId::None;
            }
        }
    }
}
