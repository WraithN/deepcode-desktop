use crate::instance::AgentInstance;
use std::collections::HashMap;
use std::sync::Arc;

pub struct InstanceRegistry {
    instances: HashMap<String, Arc<dyn AgentInstance>>,
}

impl InstanceRegistry {
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
        }
    }

    pub fn insert(&mut self, id: String, instance: Arc<dyn AgentInstance>) {
        self.instances.insert(id, instance);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn AgentInstance>> {
        self.instances.get(id).cloned()
    }

    #[allow(dead_code)]
    pub fn remove(&mut self, id: &str) {
        self.instances.remove(id);
    }

    pub fn list(&self) -> Vec<(&String, &Arc<dyn AgentInstance>)> {
        self.instances.iter().collect()
    }
}
