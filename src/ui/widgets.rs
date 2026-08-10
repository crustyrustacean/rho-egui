// src/ui/widgets.rs

/// Shared state for model/session/provider pickers, read by both `app.rs`
/// (population) and `modals.rs` (rendering).
use std::sync::RwLock;

// ── Model list ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct ModelEntry {
    pub id: String,
    pub provider: String,
    pub is_current: bool,
}

static MODELS: RwLock<Vec<ModelEntry>> = RwLock::new(Vec::new());

pub(crate) fn set_models(models: Vec<ModelEntry>) {
    *MODELS.write().unwrap() = models;
}

pub(crate) fn get_models() -> Vec<ModelEntry> {
    MODELS.read().unwrap().clone()
}

// ── Session list ──────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct SessionEntry {
    pub path: String,
    pub mtime_secs: u64,
    pub entry_count: u64,
}

static SESSIONS: RwLock<Vec<SessionEntry>> = RwLock::new(Vec::new());

pub(crate) fn set_sessions(sessions: Vec<SessionEntry>) {
    *SESSIONS.write().unwrap() = sessions;
}

pub(crate) fn get_sessions() -> Vec<SessionEntry> {
    SESSIONS.read().unwrap().clone()
}

// ── Provider list ─────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct ProviderEntry {
    pub name: String,
    pub reachable: bool,
    pub active: bool,
    pub is_external: bool,
}

static PROVIDERS: RwLock<Vec<ProviderEntry>> = RwLock::new(Vec::new());

pub(crate) fn set_providers(providers: Vec<ProviderEntry>) {
    *PROVIDERS.write().unwrap() = providers;
}

pub(crate) fn get_providers() -> Vec<ProviderEntry> {
    PROVIDERS.read().unwrap().clone()
}
