//! Recursive placeholder expansion across an entire [`UnifiedConfig`].
//!
//! Most callers should invoke [`expand_config`] once after loading layered
//! config and before handing the document off to adapters.

use crate::error::Result;
use crate::placeholder::{expand, ExpandContext};
use crate::schema::{
    McpServerConfig, ModelConfig, ProviderConfig, RulesConfig, SkillsConfig, UnifiedConfig,
};
use std::path::{Path, PathBuf};

/// Walks the configuration and expands every string value through `ctx`.
///
/// Failures short-circuit and propagate the first error; this matches the
/// strict-mode behaviour expected by `dh config apply` (lenient mode should
/// be selected on the [`ExpandContext`] before calling).
pub fn expand_config(cfg: &mut UnifiedConfig, ctx: &ExpandContext<'_>) -> Result<()> {
    expand_providers(&mut cfg.providers, ctx)?;
    expand_models(&mut cfg.models, ctx)?;
    expand_mcp(&mut cfg.mcp, ctx)?;
    expand_skills(&mut cfg.skills, ctx)?;
    expand_rules(&mut cfg.rules, ctx)?;
    Ok(())
}

fn expand_providers(
    providers: &mut std::collections::BTreeMap<String, ProviderConfig>,
    ctx: &ExpandContext<'_>,
) -> Result<()> {
    for entry in providers.values_mut() {
        expand_opt_string(&mut entry.base_url, ctx)?;
        expand_opt_string(&mut entry.api_key, ctx)?;
        for value in entry.extra.values_mut() {
            *value = expand(value, ctx)?;
        }
    }
    Ok(())
}

fn expand_models(
    models: &mut std::collections::BTreeMap<String, ModelConfig>,
    ctx: &ExpandContext<'_>,
) -> Result<()> {
    for model in models.values_mut() {
        model.provider = expand(&model.provider, ctx)?;
        model.name = expand(&model.name, ctx)?;
    }
    Ok(())
}

fn expand_mcp(mcp: &mut [McpServerConfig], ctx: &ExpandContext<'_>) -> Result<()> {
    for entry in mcp.iter_mut() {
        entry.name = expand(&entry.name, ctx)?;
        entry.command = expand(&entry.command, ctx)?;
        for arg in entry.args.iter_mut() {
            *arg = expand(arg, ctx)?;
        }
        for value in entry.env.values_mut() {
            *value = expand(value, ctx)?;
        }
    }
    Ok(())
}

fn expand_skills(skills: &mut SkillsConfig, ctx: &ExpandContext<'_>) -> Result<()> {
    for path in skills.search_paths.iter_mut() {
        *path = expand_path(path, ctx)?;
    }
    Ok(())
}

fn expand_rules(rules: &mut RulesConfig, ctx: &ExpandContext<'_>) -> Result<()> {
    for path in rules.files.iter_mut() {
        *path = expand_path(path, ctx)?;
    }
    Ok(())
}

fn expand_opt_string(value: &mut Option<String>, ctx: &ExpandContext<'_>) -> Result<()> {
    if let Some(v) = value {
        *v = expand(v, ctx)?;
    }
    Ok(())
}

fn expand_path(path: &Path, ctx: &ExpandContext<'_>) -> Result<PathBuf> {
    let raw = path.to_string_lossy();
    let expanded = expand(&raw, ctx)?;
    Ok(PathBuf::from(expanded))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placeholder::NoopKeyringResolver;

    #[test]
    fn expands_strings_across_blocks() {
        std::env::set_var("DH_EXP_TEST", "expanded");
        let mut cfg = UnifiedConfig::default();
        cfg.providers.insert(
            "p".into(),
            ProviderConfig {
                api_key: Some("k=${env:DH_EXP_TEST}".into()),
                ..Default::default()
            },
        );
        cfg.mcp.push(McpServerConfig {
            name: "m".into(),
            command: "c".into(),
            args: vec!["a-${env:DH_EXP_TEST}".into()],
            ..Default::default()
        });
        let kr = NoopKeyringResolver;
        let ctx = ExpandContext::new(&kr);
        expand_config(&mut cfg, &ctx).unwrap();
        assert_eq!(cfg.providers["p"].api_key.as_deref(), Some("k=expanded"));
        assert_eq!(cfg.mcp[0].args[0], "a-expanded");
    }
}
