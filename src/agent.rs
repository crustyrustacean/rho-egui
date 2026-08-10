// src/agent.rs

pub(crate) mod process;
pub(crate) mod protocol;

pub(crate) use process::RhoAgent;
pub(crate) use protocol::{RequestKind, RhoEvent};
