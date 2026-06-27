//! Placeholder expansion for unified config strings.
//!
//! Supports three placeholder kinds embedded in TOML string values:
//!
//! * `${env:NAME}` — read from process environment.
//! * `${keyring:KEY}` — read from a pluggable secret resolver (e.g. OS keyring).
//! * `${workspace}` — replaced with the workspace path provided at expansion
//!   time (or rejected if no workspace is supplied).
//!
//! Multiple placeholders per string are supported. Recursive expansion is
//! permitted up to [`crate::constants::MAX_PLACEHOLDER_DEPTH`] to guard
//! against accidental cycles.

use crate::constants::{
    MAX_PLACEHOLDER_DEPTH, PLACEHOLDER_CLOSE, PLACEHOLDER_KIND_ENV, PLACEHOLDER_KIND_KEYRING,
    PLACEHOLDER_KIND_SEP, PLACEHOLDER_OPEN, PLACEHOLDER_WORKSPACE,
};
use crate::error::{ConfigError, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Resolves the value of a `${keyring:KEY}` placeholder.
pub trait KeyringResolver {
    fn lookup(&self, key: &str) -> Option<String>;
}

/// Default resolver that always returns `None`. Useful when the caller has not
/// configured a keyring backend.
#[derive(Debug, Default, Clone)]
pub struct NoopKeyringResolver;

impl KeyringResolver for NoopKeyringResolver {
    fn lookup(&self, _key: &str) -> Option<String> {
        None
    }
}

/// In-memory resolver used in tests or programmatic injection.
#[derive(Debug, Default, Clone)]
pub struct InMemoryKeyringResolver {
    entries: BTreeMap<String, String>,
}

impl InMemoryKeyringResolver {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.insert(key.into(), value.into());
    }
}

impl KeyringResolver for InMemoryKeyringResolver {
    fn lookup(&self, key: &str) -> Option<String> {
        self.entries.get(key).cloned()
    }
}

/// Resolution context passed to [`expand`].
pub struct ExpandContext<'a> {
    pub workspace: Option<&'a PathBuf>,
    pub keyring: &'a dyn KeyringResolver,
    /// When false, missing env/keyring values are masked as `***` instead of
    /// raising an error. Used for `--dry-run` previews.
    pub strict: bool,
}

impl<'a> ExpandContext<'a> {
    pub fn new(keyring: &'a dyn KeyringResolver) -> Self {
        Self {
            workspace: None,
            keyring,
            strict: true,
        }
    }
    pub fn with_workspace(mut self, workspace: &'a PathBuf) -> Self {
        self.workspace = Some(workspace);
        self
    }
    pub fn lenient(mut self) -> Self {
        self.strict = false;
        self
    }
}

/// Token used in non-strict mode when a placeholder cannot be resolved.
const MASKED_VALUE: &str = "***";

/// Expands every placeholder occurrence inside `input`.
pub fn expand(input: &str, ctx: &ExpandContext<'_>) -> Result<String> {
    expand_with_depth(input, ctx, 0)
}

fn expand_with_depth(input: &str, ctx: &ExpandContext<'_>, depth: usize) -> Result<String> {
    if depth > MAX_PLACEHOLDER_DEPTH {
        return Err(ConfigError::PlaceholderDepthExceeded(MAX_PLACEHOLDER_DEPTH));
    }

    let mut out = String::with_capacity(input.len());
    let mut cursor = 0;
    while let Some(start) = input[cursor..].find(PLACEHOLDER_OPEN) {
        let abs_start = cursor + start;
        out.push_str(&input[cursor..abs_start]);

        let body_start = abs_start + PLACEHOLDER_OPEN.len();
        let close = match input[body_start..].find(PLACEHOLDER_CLOSE) {
            Some(p) => body_start + p,
            None => {
                // Unterminated placeholder: keep literal text.
                out.push_str(&input[abs_start..]);
                return Ok(out);
            }
        };

        let body = &input[body_start..close];
        let resolved = resolve_placeholder(body, ctx)?;
        // Recursively expand resolved value to support indirection like
        // `${env:HOME}/foo` -> `${workspace}/foo`.
        let nested = expand_with_depth(&resolved, ctx, depth + 1)?;
        out.push_str(&nested);
        cursor = close + 1;
    }
    out.push_str(&input[cursor..]);
    Ok(out)
}

fn resolve_placeholder(body: &str, ctx: &ExpandContext<'_>) -> Result<String> {
    if body == PLACEHOLDER_WORKSPACE {
        return resolve_workspace(ctx);
    }
    let (kind, value) = split_kind(body)?;
    match kind {
        PLACEHOLDER_KIND_ENV => resolve_env(value, ctx),
        PLACEHOLDER_KIND_KEYRING => resolve_keyring(value, ctx),
        other => Err(ConfigError::UnknownPlaceholderKind(other.to_string())),
    }
}

fn split_kind(body: &str) -> Result<(&str, &str)> {
    body.split_once(PLACEHOLDER_KIND_SEP)
        .ok_or_else(|| ConfigError::Placeholder {
            placeholder: body.to_string(),
            reason: "missing kind separator (expected `kind:value`)".into(),
        })
}

fn resolve_workspace(ctx: &ExpandContext<'_>) -> Result<String> {
    match ctx.workspace {
        Some(path) => Ok(path.to_string_lossy().to_string()),
        None if !ctx.strict => Ok(MASKED_VALUE.into()),
        None => Err(ConfigError::Placeholder {
            placeholder: PLACEHOLDER_WORKSPACE.into(),
            reason: "workspace not set in expansion context".into(),
        }),
    }
}

fn resolve_env(name: &str, ctx: &ExpandContext<'_>) -> Result<String> {
    match std::env::var(name) {
        Ok(v) => Ok(v),
        Err(_) if !ctx.strict => Ok(MASKED_VALUE.into()),
        Err(err) => Err(ConfigError::Placeholder {
            placeholder: format!("env:{name}"),
            reason: err.to_string(),
        }),
    }
}

fn resolve_keyring(key: &str, ctx: &ExpandContext<'_>) -> Result<String> {
    match ctx.keyring.lookup(key) {
        Some(v) => Ok(v),
        None if !ctx.strict => Ok(MASKED_VALUE.into()),
        None => Err(ConfigError::Placeholder {
            placeholder: format!("keyring:{key}"),
            reason: "key not found in keyring".into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(keyring: &'a dyn KeyringResolver) -> ExpandContext<'a> {
        ExpandContext::new(keyring)
    }

    #[test]
    fn passthrough_when_no_placeholder() {
        let kr = NoopKeyringResolver;
        let out = expand("plain text", &ctx(&kr)).unwrap();
        assert_eq!(out, "plain text");
    }

    #[test]
    fn expands_env() {
        std::env::set_var("DH_TEST_TOKEN", "secret");
        let kr = NoopKeyringResolver;
        let out = expand("k=${env:DH_TEST_TOKEN}", &ctx(&kr)).unwrap();
        assert_eq!(out, "k=secret");
    }

    #[test]
    fn missing_env_strict_errors() {
        let kr = NoopKeyringResolver;
        let err = expand("${env:DH_DEFINITELY_MISSING_XYZ}", &ctx(&kr)).unwrap_err();
        assert!(matches!(err, ConfigError::Placeholder { .. }));
    }

    #[test]
    fn missing_env_lenient_masks() {
        let kr = NoopKeyringResolver;
        let mut c = ctx(&kr);
        c.strict = false;
        let out = expand("${env:DH_DEFINITELY_MISSING_XYZ}", &c).unwrap();
        assert_eq!(out, MASKED_VALUE);
    }

    #[test]
    fn workspace_placeholder() {
        let kr = NoopKeyringResolver;
        let ws = PathBuf::from("/tmp/repo");
        let c = ExpandContext::new(&kr).with_workspace(&ws);
        let out = expand("${workspace}/src", &c).unwrap();
        assert_eq!(out, "/tmp/repo/src");
    }

    #[test]
    fn keyring_lookup() {
        let mut kr = InMemoryKeyringResolver::new();
        kr.insert("dh/anthropic", "sk-ant");
        let out = expand("${keyring:dh/anthropic}", &ctx(&kr)).unwrap();
        assert_eq!(out, "sk-ant");
    }

    #[test]
    fn rejects_unterminated_placeholder() {
        let kr = NoopKeyringResolver;
        let out = expand("trail ${env:X", &ctx(&kr)).unwrap();
        // Unterminated placeholders are left as-is.
        assert!(out.contains("${env:X"));
    }

    #[test]
    fn rejects_unknown_kind() {
        let kr = NoopKeyringResolver;
        let err = expand("${weird:Y}", &ctx(&kr)).unwrap_err();
        assert!(matches!(err, ConfigError::UnknownPlaceholderKind(_)));
    }
}
