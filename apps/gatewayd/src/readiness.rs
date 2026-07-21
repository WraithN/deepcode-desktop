//! Workspace-path readiness gate for `dh-gatewayd`.
//!
//! Mirrors the desktop app's [`WorkspacePathReadiness`](../platform/ready_state.html)
//! but lives in the gatewayd crate so it can run as a standalone server
//! without depending on the desktop application.
//!
//! ## Why
//!
//! Product contract:
//! 1. gatewayd POSTs runtime status to the platform.
//! 2. The platform composes `workspacePath` from
//!    `${workspace_root}/${workspace_id}/${user_id}` and returns it.
//! 3. gatewayd uses that path as the canonical working directory for
//!    opencode / claude / codex instances.
//!
//! Until step 2 succeeds the gatewayd has no platform-assigned workspace.
//! The [`SessionManager`](crate::session::SessionManager) must therefore
//! **hold** user-facing `create_agent` calls and tell the caller to wait.
//!
//! ## Behaviour matrix
//!
//! | platform configured? | workspacePath received? | is_ready() |
//! |----------------------|--------------------------|------------|
//! | no (local mode)      | n/a                      | **true**   |
//! | yes                  | no (not synced yet)      | **false**  |
//! | yes                  | yes (synced)             | **true**   |
//!
//! In local mode (platform not configured) the gatewayd falls back to the
//! caller's `work_directory` or the process CWD, exactly as before this
//! module existed.
//!
//! ## Failure handling
//!
//! On a failed status report:
//! - The cached `workspacePath` is **preserved** (we don't blank the
//!   sandbox on a transient network blip).
//! - `is_ready()` returns `false` until the next successful sync — we no
//!   longer know if the path is still in effect.
//! - Loud `ERROR` logging is rate-limited to avoid flooding the log during
//!   a sustained outage.

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
    /// True if the operator has configured platform reporting. False means
    /// "local mode" — readiness is always satisfied.
    platform_active: bool,
    /// The most recent workspace path returned by the platform, or `None` if
    /// we have not yet received one. Persists across transient network
    /// failures so the sandbox is not lost on a single blip.
    last_synced_path: Option<String>,
    /// Consecutive failure count since the last successful sync.
    failure_streak: u64,
    /// When we last emitted an `ERROR` log about failures, for cool-down.
    last_log_at: Option<Instant>,
}

impl WorkspacePathReadiness {
    /// Creates a new readiness tracker.
    ///
    /// `platform_active = true` means the operator has configured the
    /// `[platform]` section (or env vars); the gate is closed until the
    /// first successful sync.
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

    /// Returns true if user-facing operations (create agent) are allowed.
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
    pub fn mark_reported(&self, workspace_path: &str) {
        let mut inner = self.inner.lock().expect("readiness mutex poisoned");
        inner.failure_streak = 0;
        inner.last_log_at = None;

        if workspace_path.is_empty() {
            tracing::warn!(
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
            tracing::info!(
                "[Platform] Synced workspace path from platform: {workspace_path}"
            );
        }
        inner.last_synced_path = Some(workspace_path.to_string());
    }

    /// Records a failed status report.
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
            tracing::error!(
                "[Platform] Status report failed {} consecutive times; \
                 workspace_path readiness gate is closed until the next successful sync. \
                 create_agent calls will be held until then.",
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
    fn failure_after_success_closes_gate() {
        let r = WorkspacePathReadiness::new(true);
        r.mark_reported("/srv/workspace/ws1/user1");
        assert!(r.is_ready());

        r.mark_failed();
        assert!(!r.is_ready());
        assert_eq!(r.failure_streak(), 1);

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
