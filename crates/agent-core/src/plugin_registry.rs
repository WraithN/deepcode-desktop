use crate::plugin::AgentPlugin;
use std::collections::HashMap;

pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn AgentPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn AgentPlugin>) {
        self.plugins.insert(plugin.key().to_string(), plugin);
    }

    pub fn get(&self, key: &str) -> Option<&Box<dyn AgentPlugin>> {
        self.plugins.get(key)
    }

    pub fn list(&self) -> Vec<(&String, &Box<dyn AgentPlugin>)> {
        self.plugins.iter().collect()
    }
}
