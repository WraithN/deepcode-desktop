use std::collections::HashMap;
use std::sync::Mutex;
use serde_json::Value;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidecarStatus {
    Starting,
    Running { pid: u32 },
    Crashed { error: Option<String> },
    Stopped,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SidecarInstance {
    pub instance_id: String,
    pub agent_key: String,
    pub port: u16,
    pub workspace: String,
    pub status: SidecarStatus,
    pub pid: Option<u32>,
}

pub struct SidecarManager {
    pub instances: Mutex<HashMap<String, SidecarInstance>>,
    pub port_pool: Mutex<Vec<u16>>,
    pub processes: Mutex<HashMap<String, std::process::Child>>,
}

impl SidecarManager {
    pub fn new() -> Self {
        let ports: Vec<u16> = (4000..=4005).collect();
        Self {
            instances: Mutex::new(HashMap::new()),
            port_pool: Mutex::new(ports),
            processes: Mutex::new(HashMap::new()),
        }
    }

    pub fn get_available_port(&self) -> Option<u16> {
        let mut pool = self.port_pool.lock().ok()?;
        pool.pop()
    }

    pub fn release_port(&self, port: u16) {
        if let Ok(mut pool) = self.port_pool.lock() {
            if !pool.contains(&port) {
                pool.push(port);
            }
        }
    }

    pub fn instance_count(&self) -> usize {
        match self.instances.lock() {
            Ok(instances) => instances.len(),
            Err(_) => 0,
        }
    }
}

#[tauri::command]
pub fn start_sidecar(
    state: tauri::State<'_, SidecarManager>,
    instance_id: String,
    agent_key: String,
    workspace: String,
) -> Result<Value, String> {
    if state.instance_count() >= 6 {
        return Err("Maximum number of sidecar instances reached".to_string());
    }

    let port = state.get_available_port().ok_or("No available ports")?;

    let child = std::process::Command::new("opencode")
        .args(["serve", "--port", &port.to_string(), "--cors", "*"])
        .current_dir(&workspace)
        .spawn()
        .map_err(|e| format!("Failed to spawn opencode: {}", e))?;

    let pid = child.id();

    {
        let mut processes = state.processes.lock().map_err(|e| e.to_string())?;
        processes.insert(instance_id.clone(), child);
    }

    let instance = SidecarInstance {
        instance_id: instance_id.clone(),
        agent_key: agent_key.clone(),
        port,
        workspace: workspace.clone(),
        status: SidecarStatus::Running { pid },
        pid: Some(pid),
    };

    {
        let mut instances = state.instances.lock().map_err(|e| e.to_string())?;
        instances.insert(instance_id.clone(), instance);
    }

    Ok(serde_json::json!({
        "instanceId": instance_id,
        "port": port,
        "pid": pid,
        "status": "running"
    }))
}

#[tauri::command]
pub fn stop_sidecar(
    state: tauri::State<'_, SidecarManager>,
    instance_id: String,
) -> Result<(), String> {
    let (pid, port) = {
        let mut instances = state.instances.lock().map_err(|e| e.to_string())?;
        let instance = instances.remove(&instance_id).ok_or("Instance not found")?;
        (instance.pid, instance.port)
    };

    if let Some(pid) = pid {
        std::process::Command::new("kill")
            .arg(pid.to_string())
            .output()
            .map_err(|e| format!("Failed to kill process: {}", e))?;
    }

    {
        let mut processes = state.processes.lock().map_err(|e| e.to_string())?;
        processes.remove(&instance_id);
    }

    state.release_port(port);

    Ok(())
}

#[tauri::command]
pub fn get_sidecar_status(
    state: tauri::State<'_, SidecarManager>,
    instance_id: String,
) -> Result<Value, String> {
    let instances = state.instances.lock().map_err(|e| e.to_string())?;
    let instance = instances.get(&instance_id).ok_or("Instance not found")?;
    Ok(serde_json::to_value(instance).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn check_opencode_installed() -> Result<String, String> {
    #[cfg(not(target_os = "windows"))]
    {
        let output = std::process::Command::new("which")
            .arg("opencode")
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            let path = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
            Ok(path.trim().to_string())
        } else {
            Err("opencode is not installed".to_string())
        }
    }
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("where")
            .arg("opencode")
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            let path = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
            Ok(path.trim().to_string())
        } else {
            Err("opencode is not installed".to_string())
        }
    }
}
