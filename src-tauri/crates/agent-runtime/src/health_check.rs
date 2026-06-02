use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::process::Child;
use tokio::time::{interval, Duration};

pub struct HealthChecker {
    processes: Arc<Mutex<HashMap<String, Child>>>,
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register(&self, instance_id: String, child: Child) {
        if let Ok(mut map) = self.processes.lock() {
            map.insert(instance_id, child);
        }
    }

    pub fn unregister(&self, instance_id: &str) {
        if let Ok(mut map) = self.processes.lock() {
            map.remove(instance_id);
        }
    }

    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(5));
            loop {
                ticker.tick().await;
                let mut to_remove = Vec::new();
                if let Ok(mut map) = self.processes.lock() {
                    for (id, child) in map.iter_mut() {
                        match child.try_wait() {
                            Ok(Some(_status)) => {
                                to_remove.push(id.clone());
                            }
                            Ok(None) => {}
                            Err(_) => {
                                to_remove.push(id.clone());
                            }
                        }
                    }
                    for id in &to_remove {
                        map.remove(id);
                    }
                }
            }
        });
    }
}
