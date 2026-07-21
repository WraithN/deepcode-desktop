//! Agent Runtime status collector.
//!
//! Gathers process-level metrics (CPU, memory, uptime) and aggregates the
//! status of all managed agent instances into the payload expected by the
//! DeepHarness Enterprise Platform:
//!
//! ```text
//! POST /api/v1/agent-runtimes/{runtimeId}/status
//! ```

use crate::platform::payload::{RuntimeAgentStatus, RuntimeStatusReport};
use agent_core::instance::InstanceStatus;
use agent_core::models::InstanceInfo;
use dh_config::PlatformConfig;
use std::sync::Mutex;
use std::time::Instant;
use sysinfo::{Pid, System};

/// Maximum sensible CPU percentage to report.
const MAX_CPU_PERCENT: f64 = 100.0;

/// Default value for agent fields that are not yet collected.
const DEFAULT_AGENT_VERSION: &str = "";
const DEFAULT_LAST_ACTIVE: &str = "";

/// Collector that tracks the desktop application's runtime status.
///
/// Holds the application start instant and a [`System`] snapshot used to
/// refresh process metrics on demand.
pub struct RuntimeStatusCollector {
    start_time: Instant,
    system: Mutex<System>,
}

impl RuntimeStatusCollector {
    /// Creates a new collector recording the current time as the runtime start.
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            system: Mutex::new(System::new_all()),
        }
    }

    /// Builds a [`RuntimeStatusReport`] from the provided configuration and
    /// agent instances.
    ///
    /// Returns `None` when runtime reporting is not active (missing runtime id
    /// or tenant id) or when the instance list cannot be inspected.
    pub fn build_report(&self, config: &PlatformConfig, instances: &[InstanceInfo], workspace_path: &str) -> Option<RuntimeStatusReport> {
        if !config.is_runtime_reporting_active() {
            return None;
        }

        let (cpu_percent, mem_percent, sandbox_spec) = self.collect_metrics();
        let agent_statuses = instances.iter().map(map_instance_to_agent_status).collect();
        let overall_status = aggregate_runtime_status(instances);

        Some(RuntimeStatusReport {
            workspace_id: config.workspace_id.clone().unwrap_or_default(),
            workspace_path: workspace_path.to_string(),
            user_id: config.user_id.clone().unwrap_or_default(),
            status: overall_status,
            uptime_seconds: self.start_time.elapsed().as_secs(),
            cpu_percent,
            mem_percent,
            sandbox_spec,
            agents: agent_statuses,
        })
    }

    /// Refreshes process metrics and returns the current process CPU/memory
    /// usage percentages plus the auto-detected sandbox specification.
    fn collect_metrics(&self) -> (f64, f64, String) {
        let Ok(mut system) = self.system.lock() else {
            return (0.0, 0.0, String::new());
        };

        system.refresh_all();
        let sandbox_spec = detect_sandbox_spec(&system);
        let current_pid = Pid::from(std::process::id() as usize);
        let Some(process) = system.process(current_pid) else {
            return (0.0, 0.0, sandbox_spec);
        };

        let cpu_percent = process.cpu_usage().clamp(0.0_f32, MAX_CPU_PERCENT as f32) as f64;
        let total_memory = system.total_memory();
        let mem_percent = if total_memory == 0 {
            0.0
        } else {
            let used_memory = process.memory();
            (used_memory as f64 / total_memory as f64) * 100.0
        };

        (cpu_percent, mem_percent, sandbox_spec)
    }
}

/// Detects the runtime sandbox specification from system information.
///
/// Format: `"{physical_cores}C / {total_memory_gb}G"`
fn detect_sandbox_spec(system: &System) -> String {
    let cpu_count = system.physical_core_count().unwrap_or_else(|| system.cpus().len());
    let memory_gb = system.total_memory() / 1024 / 1024 / 1024;
    format!("{cpu_count}C / {memory_gb}G")
}

impl Default for RuntimeStatusCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps an agent instance info to the enterprise platform agent status model.
fn map_instance_to_agent_status(info: &InstanceInfo) -> RuntimeAgentStatus {
    RuntimeAgentStatus {
        agent_type: info.agent_key.clone(),
        name: info.name.clone(),
        status: map_instance_status(&info.status),
        calls_today: 0,
        version: DEFAULT_AGENT_VERSION.to_string(),
        last_active: DEFAULT_LAST_ACTIVE.to_string(),
    }
}

/// Maps the internal instance status to the platform agent status enum.
///
/// Internal status values:
/// - `stopped` / `starting` / `running { pid }` / `crashed(reason)`
///
/// Platform agent status values:
/// - `running` / `error` / `idle` / `stopped`
fn map_instance_status(status: &InstanceStatus) -> String {
    match status {
        InstanceStatus::Running { .. } => "running",
        InstanceStatus::Crashed(_) => "error",
        InstanceStatus::Starting => "idle",
        InstanceStatus::Stopped => "stopped",
    }
    .to_string()
}

/// Aggregates individual agent statuses into a single runtime status.
///
/// Rules:
/// - If any agent is crashed, the runtime is `error`.
/// - If any agent is running or starting, the runtime is `running`.
/// - Otherwise (all stopped), the runtime is `stopped`.
fn aggregate_runtime_status(instances: &[InstanceInfo]) -> String {
    if instances.iter().any(|i| matches!(i.status, InstanceStatus::Crashed(_))) {
        return "error".to_string();
    }

    if instances
        .iter()
        .any(|i| matches!(i.status, InstanceStatus::Running { .. } | InstanceStatus::Starting))
    {
        return "running".to_string();
    }

    "stopped".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::instance::InstanceStatus;
    use agent_core::models::InstanceInfo;

    fn instance_with_status(status: InstanceStatus) -> InstanceInfo {
        InstanceInfo {
            id: "i1".to_string(),
            agent_key: "opencode".to_string(),
            name: "opencode-main".to_string(),
            work_directory: "/tmp".to_string(),
            status,
            endpoint: None,
        }
    }

    #[test]
    fn aggregate_runtime_status_prefers_error() {
        let instances = vec![
            instance_with_status(InstanceStatus::Running { pid: 1 }),
            instance_with_status(InstanceStatus::Crashed("boom".to_string())),
        ];
        assert_eq!(aggregate_runtime_status(&instances), "error");
    }

    #[test]
    fn aggregate_runtime_status_running_when_any_active() {
        let instances = vec![
            instance_with_status(InstanceStatus::Stopped),
            instance_with_status(InstanceStatus::Running { pid: 1 }),
        ];
        assert_eq!(aggregate_runtime_status(&instances), "running");
    }

    #[test]
    fn aggregate_runtime_status_stopped_when_all_stopped() {
        let instances = vec![
            instance_with_status(InstanceStatus::Stopped),
            instance_with_status(InstanceStatus::Stopped),
        ];
        assert_eq!(aggregate_runtime_status(&instances), "stopped");
    }

    #[test]
    fn map_instance_status_values() {
        assert_eq!(map_instance_status(&InstanceStatus::Running { pid: 42 }), "running");
        assert_eq!(map_instance_status(&InstanceStatus::Crashed("x".to_string())), "error");
        assert_eq!(map_instance_status(&InstanceStatus::Starting), "idle");
        assert_eq!(map_instance_status(&InstanceStatus::Stopped), "stopped");
    }

    #[test]
    fn detect_sandbox_spec_format() {
        let system = System::new_all();
        let spec = detect_sandbox_spec(&system);
        assert!(spec.contains("C / "), "sandbox spec should contain 'C / ', got: {spec}");
        assert!(spec.ends_with('G'), "sandbox spec should end with 'G', got: {spec}");
        assert!(
            spec.chars().next().unwrap_or(' ').is_ascii_digit(),
            "sandbox spec should start with a digit, got: {spec}"
        );
    }
}
