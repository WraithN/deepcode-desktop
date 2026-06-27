//! Validation pass for [`UnifiedConfig`].
//!
//! Reports as many issues as possible in a single run instead of bailing out
//! on the first error. The returned [`ValidationReport`] has a convenience
//! [`into_result`] helper for callers that want a `Result`-shaped API.

use crate::error::{ConfigError, Result};
use crate::schema::{McpServerConfig, ModelConfig, UnifiedConfig};
use std::collections::BTreeSet;

/// A single validation issue. Errors are blocking; warnings are advisory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    Error(String),
    Warning(String),
}

impl Issue {
    pub fn message(&self) -> &str {
        match self {
            Issue::Error(m) | Issue::Warning(m) => m,
        }
    }
    pub fn is_error(&self) -> bool {
        matches!(self, Issue::Error(_))
    }
}

/// Aggregated validation findings.
#[derive(Debug, Default, Clone)]
pub struct ValidationReport {
    pub issues: Vec<Issue>,
}

impl ValidationReport {
    pub fn is_ok(&self) -> bool {
        !self.issues.iter().any(Issue::is_error)
    }

    pub fn errors(&self) -> impl Iterator<Item = &str> {
        self.issues
            .iter()
            .filter_map(|i| if i.is_error() { Some(i.message()) } else { None })
    }

    pub fn warnings(&self) -> impl Iterator<Item = &str> {
        self.issues
            .iter()
            .filter_map(|i| if !i.is_error() { Some(i.message()) } else { None })
    }

    /// Converts the report into a `Result`. Multiple errors are joined with
    /// `; ` to keep the surface a single [`ConfigError::Validation`].
    pub fn into_result(self) -> Result<()> {
        if self.is_ok() {
            return Ok(());
        }
        let combined = self
            .errors()
            .collect::<Vec<_>>()
            .join("; ");
        Err(ConfigError::Validation(combined))
    }
}

/// Validates the merged unified config.
pub fn validate(cfg: &UnifiedConfig) -> ValidationReport {
    let mut report = ValidationReport::default();
    validate_models(cfg, &mut report);
    validate_mcp(cfg, &mut report);
    validate_skills(cfg, &mut report);
    report
}

// ───── individual checks ─────

fn validate_models(cfg: &UnifiedConfig, report: &mut ValidationReport) {
    if cfg.models.is_empty() {
        return;
    }
    if !cfg.models.contains_key("default") {
        report.issues.push(Issue::Warning(
            "no `[models.default]` defined; adapters may fall back to their built-in default"
                .into(),
        ));
    }
    for (name, model) in &cfg.models {
        validate_model_entry(name, model, cfg, report);
    }
}

fn validate_model_entry(
    name: &str,
    model: &ModelConfig,
    cfg: &UnifiedConfig,
    report: &mut ValidationReport,
) {
    if model.provider.is_empty() {
        report
            .issues
            .push(Issue::Error(format!("models.{name}: provider is empty")));
        return;
    }
    if model.name.is_empty() {
        report
            .issues
            .push(Issue::Error(format!("models.{name}: model name is empty")));
    }
    if !cfg.providers.contains_key(&model.provider) {
        report.issues.push(Issue::Error(format!(
            "models.{name}: provider `{}` is not defined under [providers]",
            model.provider
        )));
    }
}

fn validate_mcp(cfg: &UnifiedConfig, report: &mut ValidationReport) {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for entry in &cfg.mcp {
        validate_mcp_entry(entry, report);
        if !seen.insert(entry.name.as_str()) {
            report.issues.push(Issue::Error(format!(
                "duplicate mcp server name: `{}`",
                entry.name
            )));
        }
    }
}

fn validate_mcp_entry(entry: &McpServerConfig, report: &mut ValidationReport) {
    if entry.name.is_empty() {
        report
            .issues
            .push(Issue::Error("mcp entry has empty `name`".into()));
    }
    if entry.command.is_empty() {
        report.issues.push(Issue::Error(format!(
            "mcp.{}: `command` is empty",
            entry.name
        )));
    }
}

fn validate_skills(cfg: &UnifiedConfig, report: &mut ValidationReport) {
    for path in &cfg.skills.search_paths {
        if path.as_os_str().is_empty() {
            report
                .issues
                .push(Issue::Error("skills.search_paths contains empty entry".into()));
        }
    }
    for name in &cfg.skills.enabled {
        if name.trim().is_empty() {
            report
                .issues
                .push(Issue::Error("skills.enabled contains empty entry".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{McpServerConfig, ModelConfig, ProviderConfig};

    fn cfg_with_provider() -> UnifiedConfig {
        let mut cfg = UnifiedConfig::default();
        cfg.providers
            .insert("ds".into(), ProviderConfig::default());
        cfg.models.insert(
            "default".into(),
            ModelConfig {
                provider: "ds".into(),
                name: "deepseek-coder".into(),
                ..Default::default()
            },
        );
        cfg
    }

    #[test]
    fn ok_for_minimal_valid_config() {
        let cfg = cfg_with_provider();
        let report = validate(&cfg);
        assert!(report.is_ok(), "errors: {:?}", report.issues);
    }

    #[test]
    fn errors_on_unknown_provider() {
        let mut cfg = UnifiedConfig::default();
        cfg.models.insert(
            "default".into(),
            ModelConfig {
                provider: "missing".into(),
                name: "x".into(),
                ..Default::default()
            },
        );
        let report = validate(&cfg);
        assert!(!report.is_ok());
        assert!(report.errors().any(|m| m.contains("missing")));
    }

    #[test]
    fn errors_on_duplicate_mcp_name() {
        let mut cfg = cfg_with_provider();
        cfg.mcp.push(McpServerConfig {
            name: "fs".into(),
            command: "npx".into(),
            ..Default::default()
        });
        cfg.mcp.push(McpServerConfig {
            name: "fs".into(),
            command: "node".into(),
            ..Default::default()
        });
        let report = validate(&cfg);
        assert!(!report.is_ok());
        assert!(report.errors().any(|m| m.contains("duplicate mcp")));
    }

    #[test]
    fn warns_when_default_model_missing_but_models_present() {
        let mut cfg = UnifiedConfig::default();
        cfg.providers
            .insert("p".into(), ProviderConfig::default());
        cfg.models.insert(
            "review".into(),
            ModelConfig {
                provider: "p".into(),
                name: "x".into(),
                ..Default::default()
            },
        );
        let report = validate(&cfg);
        assert!(report.is_ok());
        assert!(report.warnings().any(|m| m.contains("default")));
    }

    #[test]
    fn into_result_collects_errors() {
        let mut cfg = UnifiedConfig::default();
        cfg.models.insert(
            "default".into(),
            ModelConfig {
                provider: "missing".into(),
                name: String::new(),
                ..Default::default()
            },
        );
        let err = validate(&cfg).into_result().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("model name is empty"));
        assert!(msg.contains("not defined"));
    }
}
