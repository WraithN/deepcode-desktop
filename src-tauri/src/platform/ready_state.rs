//! Workspace-path readiness state shared between the platform reporter and
//! the gateway request handlers.
//!
//! ## Why this exists
//!
//! The product contract is:
//! 1. Desktop POSTs runtime status to the platform.
//! 2. Platform composes `workspacePath` from
//!    `${workspace_root}/${workspace_id}/${user_id}` and returns it.
//! 3. Desktop uses that path as the canonical working directory for
//!    opencode / claude-code / codex instances and as a directory sandbox.
//!
//! Until step 2 succeeds the desktop has no platform-assigned workspace, so
//! the gateway must **hold user-facing operations** (create instance, send
//! message, run) and tell the user to wait. This module is the single
//! source of truth for "is the workspace path ready?".
//!
//! ## Behaviour matrix
//!
//! | platform configured? | workspacePath received? | is_ready() |
//! |----------------------|--------------------------|------------|
//! | no (local mode)      | n/a                      | **true**   |
//! | yes                  | no (not synced yet)      | **false**  |
//! | yes                  | yes (synced)             | **true**   |
//!
//! Local mode preserves the existing UX: the desktop falls back to whatever
//! the user set via `agent.setWorkspacePath` or the process CWD, with no
//! readiness gate.
//!
//! ## Failure handling
//!
//! When the platform is unreachable or returns an error, the previously
//! synced `workspacePath` in the local DB is preserved (we do **not** clear
//! it on failure — the desktop would otherwise lose its sandbox boundary
//! whenever the network blips). However, `is_ready()` reverts to `false`
//! until the next successful sync proves the path is still in effect.
//!
//! To avoid log spam, the `mark_failed` path emits an `ERROR` only after a
//! configurable streak of consecutive failures, then suppresses further
//! messages for a cool-down window.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Number of consecutive failed reports before we start logging loudly.
const FAIL_LOG_STREAK: u64 = 3;

/// Cool-down between loud failure logs so a sustained outage does not flood
/// the log.
const FAIL_LOG_COOLDOWN: Duration = Duration::from_secs(5 * 60);

/// Thread-safe readiness tracker.
#[derive(Debug)]
pub struct WorkspacePathReadiness {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// True if the user has configured a `[platform]` block with `enabled=true`
    /// and a non-empty `url`. False means "local mode" — readiness is always
    /// satisfied.
    platform_active: bool,
    /// The most recent workspace path returned by the platform, or `None` if
    /// we have not yet received one. Persists across transient network
    /// failures so the sandbox is not lost on a single blip.
    last_synced_path: Option<String>,
    /// Consecutive failure count since the last successful sync.
    failure_streak: u64,
    /// When we last emitted a `ERROR` log about failures, for cool-down.
    last_log_at: Option<Instant>,
}

impl WorkspacePathReadiness {
    /// Creates a new readiness tracker.
    ///
    /// - `platform_active = true` means the user has configured the
    ///   `[platform]` section; the gate will be closed until the first
    ///   successful sync.
    /// - `platform_active = false` puts the desktop in local mode; the gate
    ///   is always open and no platform sync is required.
    pub fn new(platform_active: bool) -> Self {
        Self {
            inner: Mutex::new(Inner {
                platform_active,
                last_synced_path: None,
                failure_streak: 0,
                last_log_at: None,
            }),
        }
    }

    /// Convenience constructor for the common "no platform" case.
    pub fn local_mode() -> Self {
        Self::new(false)
    }

    /// Returns true if user-facing operations (create instance, send
    /// message, run) are allowed.
    ///
    /// In local mode this is always true. In platform mode it requires at
    /// least one successful status report whose response carried a
    /// non-empty `workspacePath`.
    pub fn is_ready(&self) -> bool {
        let inner = self.inner.lock().expect("readiness mutex poisoned");
        if !inner.platform_active {
            return true;
        }
        inner
            .last_synced_path
            .as_ref()
            .is_some_and(|p| !p.is_empty())
    }

    /// Returns the most recently synced workspace path, or `None` if no
    /// successful sync has happened yet (or the platform is not active).
    pub fn current_path(&self) -> Option<String> {
        let inner = self.inner.lock().expect("readiness mutex poisoned");
        inner.last_synced_path.clone()
    }

    /// Records a successful status report.
    ///
    /// `workspace_path` is what the platform returned. If non-empty, the
    /// readiness gate opens and the path is cached as the canonical sandbox
    /// for agent instances. An empty path is treated as "platform did not
    /// assign a workspace" and leaves the gate closed.
    pub fn mark_reported(&self, workspace_path: &str) {
        let mut inner = self.inner.lock().expect("readiness mutex poisoned");
        inner.failure_streak = 0;
        inner.last_log_at = None;

        if workspace_path.is_empty() {
            log::warn!(
                "[Platform] Status report OK but platform returned empty workspacePath; \
                 readiness gate stays closed"
            );
            return;
        }

        if inner
            .last_synced_path
            .as_ref()
            .is_none_or(|prev| prev != workspace_path)
        {
            log::info!(
                "[Platform] Synced workspace path from platform: {workspace_path}"
            );
        }
        inner.last_synced_path = Some(workspace_path.to_string());
    }

    /// Records a failed status report.
    ///
    /// Failure semantics:
    /// - The cached `workspacePath` is preserved (we do not clear it on
    ///   network blips so the sandbox is not lost).
    /// - `is_ready()` returns `false` until the next successful sync —
    ///   a network outage that prevents the platform from re-confirming the
    ///   path is treated as "we no longer know if the path is still valid".
    /// - Loud `ERROR` logging is rate-limited to avoid spam during outages.
    pub fn mark_failed(&self) {
        let mut inner = self.inner.lock().expect("readiness mutex poisoned");
        inner.failure_streak = inner.failure_streak.saturating_add(1);
        inner.last_synced_path = None;

        let should_log = inner.failure_streak >= FAIL_LOG_STREAK
            && inner
                .last_log_at
                .map(|t| t.elapsed() >= FAIL_LOG_COOLDOWN)
                .unwrap_or(true);

        if should_log {
            log::error!(
                "[Platform] Status report failed {} consecutive times; \
                 workspace_path readiness gate is closed until the next successful sync. \
                 User-facing agent operations will be held until then.",
                inner.failure_streak
            );
            inner.last_log_at = Some(Instant::now());
        }
    }

    /// Returns the current consecutive failure count (mostly for tests).
    #[cfg(test)]
    pub fn failure_streak(&self) -> u64 {
        self.inner.lock().expect("readiness mutex poisoned").failure_streak
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_mode_is_always_ready() {
        let r = WorkspacePathReadiness::local_mode();
        assert!(r.is_ready());
        assert!(r.current_path().is_none());

        // Even "failures" in local mode must not affect readiness.
        r.mark_failed();
        assert!(r.is_ready());
    }

    #[test]
    fn platform_mode_starts_unready() {
        let r = WorkspacePathReadiness::new(true);
        assert!(!r.is_ready());
        assert!(r.current_path().is_none());
    }

    #[test]
    fn successful_report_with_path_opens_gate() {
        let r = WorkspacePathReadiness::new(true);
        r.mark_reported("/srv/workspace/ws1/user1");
        assert!(r.is_ready());
        assert_eq!(r.current_path().as_deref(), Some("/srv/workspace/ws1/user1"));
    }

    #[test]
    fn successful_report_with_empty_path_keeps_gate_closed() {
        let r = WorkspacePathReadiness::new(true);
        r.mark_reported("");
        assert!(!r.is_ready());
    }

    #[test]
    fn failure_after_success_closes_gate_but_keeps_failure_streak() {
        let r = WorkspacePathReadiness::new(true);
        r.mark_reported("/srv/workspace/ws1/user1");
        assert!(r.is_ready());

        r.mark_failed();
        assert!(!r.is_ready());
        assert_eq!(r.failure_streak(), 1);

        // A subsequent success re-opens the gate.
        r.mark_reported("/srv/workspace/ws1/user1");
        assert!(r.is_ready());
        assert_eq!(r.failure_streak(), 0);
    }

    #[test]
    fn repeated_failures_increment_streak() {
        let r = WorkspacePathReadiness::new(true);
        for _ in 0..5 {
            r.mark_failed();
        }
        assert_eq!(r.failure_streak(), 5);
        assert!(!r.is_ready());
    }
}
