//! Layered merge logic.
//!
//! Combines configuration documents from multiple scopes into a single
//! [`UnifiedConfig`]. Merge order from lowest to highest precedence:
//!
//! 1. Global config (`~/.config/dh/config.toml`)
//! 2. Profile override (`~/.config/dh/profiles/<name>.toml`)
//! 3. Project config (`<repo>/.dh/config.toml`)
//!
//! Scalar fields are overwritten by later layers. List fields default to
//! append semantics; map fields are merged key-by-key.

use crate::schema::{
    McpServerConfig, ModelConfig, ProviderConfig, RulesConfig, SkillsConfig, UnifiedConfig,
};
use std::collections::BTreeMap;

/// Merges `over` into `base`, returning the combined configuration.
///
/// Precedence: fields present in `over` win over `base`.
pub fn merge(mut base: UnifiedConfig, over: UnifiedConfig) -> UnifiedConfig {
    if over.default_profile.is_some() {
        base.default_profile = over.default_profile;
    }
    base.providers = merge_providers(base.providers, over.providers);
    base.models = merge_models(base.models, over.models);
    base.mcp = merge_mcp(base.mcp, over.mcp);
    base.skills = merge_skills(base.skills, over.skills);
    base.rules = merge_rules(base.rules, over.rules);
    base
}

/// Merges any number of layers from low to high precedence.
pub fn merge_all<I>(layers: I) -> UnifiedConfig
where
    I: IntoIterator<Item = UnifiedConfig>,
{
    layers.into_iter().fold(UnifiedConfig::default(), merge)
}

// ───── private helpers ─────

fn merge_providers(
    mut base: BTreeMap<String, ProviderConfig>,
    over: BTreeMap<String, ProviderConfig>,
) -> BTreeMap<String, ProviderConfig> {
    for (key, value) in over {
        match base.remove(&key) {
            Some(existing) => {
                base.insert(key, merge_provider_entry(existing, value));
            }
            None => {
                base.insert(key, value);
            }
        }
    }
    base
}

fn merge_provider_entry(mut base: ProviderConfig, over: ProviderConfig) -> ProviderConfig {
    if over.base_url.is_some() {
        base.base_url = over.base_url;
    }
    if over.api_key.is_some() {
        base.api_key = over.api_key;
    }
    base.extra.extend(over.extra);
    base
}

fn merge_models(
    mut base: BTreeMap<String, ModelConfig>,
    over: BTreeMap<String, ModelConfig>,
) -> BTreeMap<String, ModelConfig> {
    for (key, value) in over {
        base.insert(key, value);
    }
    base
}

/// MCP servers are matched by `name`; later layers override earlier ones with
/// the same name; new names are appended preserving insertion order.
fn merge_mcp(base: Vec<McpServerConfig>, over: Vec<McpServerConfig>) -> Vec<McpServerConfig> {
    let mut result: Vec<McpServerConfig> = base;
    for entry in over {
        match result.iter().position(|e| e.name == entry.name) {
            Some(idx) => result[idx] = entry,
            None => result.push(entry),
        }
    }
    result
}

fn merge_skills(mut base: SkillsConfig, over: SkillsConfig) -> SkillsConfig {
    for path in over.search_paths {
        if !base.search_paths.contains(&path) {
            base.search_paths.push(path);
        }
    }
    for skill in over.enabled {
        if !base.enabled.contains(&skill) {
            base.enabled.push(skill);
        }
    }
    base
}

fn merge_rules(mut base: RulesConfig, over: RulesConfig) -> RulesConfig {
    for path in over.files {
        if !base.files.contains(&path) {
            base.files.push(path);
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn provider(api_key: &str) -> ProviderConfig {
        ProviderConfig {
            api_key: Some(api_key.to_string()),
            ..Default::default()
        }
    }

    fn mcp(name: &str, command: &str) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            command: command.to_string(),
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn provider_override() {
        let mut base = UnifiedConfig::default();
        base.providers.insert("deepseek".into(), provider("old"));
        let mut over = UnifiedConfig::default();
        over.providers.insert("deepseek".into(), provider("new"));

        let merged = merge(base, over);
        assert_eq!(
            merged.providers["deepseek"].api_key.as_deref(),
            Some("new")
        );
    }

    #[test]
    fn mcp_same_name_overrides_others_appended() {
        let mut base = UnifiedConfig::default();
        base.mcp.push(mcp("fs", "old-cmd"));
        base.mcp.push(mcp("git", "git"));

        let mut over = UnifiedConfig::default();
        over.mcp.push(mcp("fs", "new-cmd"));
        over.mcp.push(mcp("search", "rg"));

        let merged = merge(base, over);
        assert_eq!(merged.mcp.len(), 3);
        assert_eq!(merged.mcp[0].command, "new-cmd"); // overridden in place
        assert_eq!(merged.mcp[1].name, "git");
        assert_eq!(merged.mcp[2].name, "search");
    }

    #[test]
    fn skills_dedup_append() {
        let mut base = UnifiedConfig::default();
        base.skills.search_paths.push(PathBuf::from("/a"));
        base.skills.enabled.push("review".into());
        let mut over = UnifiedConfig::default();
        over.skills.search_paths.push(PathBuf::from("/a")); // dup
        over.skills.search_paths.push(PathBuf::from("/b"));
        over.skills.enabled.push("refactor".into());

        let merged = merge(base, over);
        assert_eq!(merged.skills.search_paths.len(), 2);
        assert_eq!(merged.skills.enabled, vec!["review", "refactor"]);
    }

    #[test]
    fn merge_all_layered() {
        let mut g = UnifiedConfig::default();
        g.default_profile = Some("g".into());
        g.providers.insert("p".into(), provider("g"));
        let mut p = UnifiedConfig::default();
        p.providers.insert("p".into(), provider("p"));
        let mut proj = UnifiedConfig::default();
        proj.providers.insert("p".into(), provider("proj"));

        let merged = merge_all([g, p, proj]);
        assert_eq!(merged.default_profile.as_deref(), Some("g"));
        assert_eq!(merged.providers["p"].api_key.as_deref(), Some("proj"));
    }
}
