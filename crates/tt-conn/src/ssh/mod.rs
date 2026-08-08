//! The SSH transport, and the two files a Linux user expects it to read.
//!
//! Tera Term keeps its own host-key and identity stores, in its own install
//! directory, in its own dialogs. That is the single most-cited reason a
//! Linux-side user bounces off it: `~/.ssh/config` already says which key and
//! which user go with which host, `~/.ssh/known_hosts` already knows who the
//! machines are, and a client that ignores both is asking to be configured
//! twice. `PLAN.md` calls this "a major Linux adoption lever"; it is here
//! before the transport because the transport needs both to answer its first
//! question, which is whether to trust the far end at all.

mod conn;
pub mod known_hosts;
mod wakeup;

pub use conn::{
    default_identities, AuthPrompt, AuthPromptKind, HostKeyDecision, HostKeyPrompt, Prompt,
    SshConn, SshConnect, SshParams, Step,
};
pub use known_hosts::{HostKey, HostKeyRef, KnownHosts, Verdict};
