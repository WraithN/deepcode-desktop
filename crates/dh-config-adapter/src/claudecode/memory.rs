//! Builds the `CLAUDE.md` rules section and skill slash-command files.

use crate::constants::{MARKDOWN_BEGIN_MARKER, MARKDOWN_END_MARKER};
use crate::error::{AdapterError, Result};
use dh_config::UnifiedConfig;
use std::fs;
use std::path::{Path, PathBuf};

const RULES_HEADER: &str = "## Engineering Rules (managed by dh)";

/// Renders the new CLAUDE.md content by replacing the dh-managed block while
/// preserving the rest of the file.
pub fn render_memory(workspace: &Path, cfg: &UnifiedConfig) -> Result<String> {
    let existing = read_existing_memory(workspace)?;
    let managed = build_managed_block(cfg)?;
    Ok(splice_managed_block(&existing, &managed))
}

/// Resolves and reads each rule file referenced by the unified config and
/// stitches them into a markdown section delimited by the dh markers.
fn build_managed_block(cfg: &UnifiedConfig) -> Result<String> {
    let mut body = String::new();
    body.push_str(MARKDOWN_BEGIN_MARKER);
    body.push('\n');
    body.push_str(RULES_HEADER);
    body.push_str("\n\n");
    for path in &cfg.rules.files {
        append_rule_file(&mut body, path)?;
    }
    if cfg.rules.files.is_empty() {
        body.push_str("_No rules configured._\n");
    }
    body.push_str(MARKDOWN_END_MARKER);
    body.push('\n');
    Ok(body)
}

fn append_rule_file(body: &mut String, path: &Path) -> Result<()> {
    if !path.exists() {
        // Missing rule files are demoted to a placeholder rather than an
        // error so apply remains usable while authors stage edits.
        body.push_str(&format!(
            "<!-- dh: missing rule file: {} -->\n",
            path.display()
        ));
        return Ok(());
    }
    let content = fs::read_to_string(path).map_err(|source| AdapterError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    body.push_str(content.trim_end());
    body.push_str("\n\n");
    Ok(())
}

fn read_existing_memory(workspace: &Path) -> Result<String> {
    let path = workspace.join("CLAUDE.md");
    if !path.exists() {
        return Ok(String::new());
    }
    fs::read_to_string(&path).map_err(|source| AdapterError::Io { path, source })
}

/// Replaces the contents between the begin/end markers with `managed`. If the
/// markers are missing, appends `managed` to the document.
fn splice_managed_block(existing: &str, managed: &str) -> String {
    let combined = if let (Some(start), Some(end)) = find_marker_range(existing) {
        let mut out = String::with_capacity(existing.len() + managed.len());
        out.push_str(&existing[..start]);
        out.push_str(managed);
        out.push_str(&existing[end..]);
        out
    } else {
        let mut out = existing.trim_end().to_string();
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(managed);
        out
    };
    // Normalise trailing whitespace so repeated applies are idempotent.
    let mut normalised = combined.trim_end().to_string();
    normalised.push('\n');
    normalised
}

fn find_marker_range(existing: &str) -> (Option<usize>, Option<usize>) {
    let begin = existing.find(MARKDOWN_BEGIN_MARKER);
    let end = existing
        .find(MARKDOWN_END_MARKER)
        .map(|pos| pos + MARKDOWN_END_MARKER.len());
    match (begin, end) {
        (Some(b), Some(e)) if e > b => (Some(b), Some(e)),
        _ => (None, None),
    }
}

/// Builds a slash-command markdown body for a skill name.
pub fn render_skill_command(skill_name: &str) -> (PathBuf, String) {
    let path = PathBuf::from(format!("{skill_name}.md"));
    let body = format!(
        "---\nname: {skill_name}\ndescription: Loaded by dh from skill `{skill_name}`\n---\n\n_See `dh config skills show {skill_name}` for the full instructions._\n"
    );
    (path, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn splices_into_existing_markers() {
        let existing = "# Project\n\n<!-- dh:begin -->\nold\n<!-- dh:end -->\n\nFooter\n";
        let managed = "<!-- dh:begin -->\nnew\n<!-- dh:end -->\n";
        let out = splice_managed_block(existing, managed);
        assert!(out.contains("new"));
        assert!(!out.contains("old"));
        assert!(out.contains("Footer"));
    }

    #[test]
    fn appends_when_no_markers() {
        let existing = "# Project\n\nbody";
        let managed = "<!-- dh:begin -->\nx\n<!-- dh:end -->\n";
        let out = splice_managed_block(existing, managed);
        assert!(out.contains("# Project"));
        assert!(out.ends_with("<!-- dh:end -->\n"));
    }

    #[test]
    fn splice_is_idempotent() {
        let managed = "<!-- dh:begin -->\nbody\n<!-- dh:end -->\n";
        let first = splice_managed_block("", managed);
        let second = splice_managed_block(&first, managed);
        assert_eq!(first, second, "splice must be idempotent");
    }

    #[test]
    fn missing_rule_file_yields_placeholder() {
        let mut cfg = UnifiedConfig::default();
        cfg.rules.files.push(PathBuf::from("/no/such/file.md"));
        let block = build_managed_block(&cfg).unwrap();
        assert!(block.contains("missing rule file"));
    }

    #[test]
    fn render_memory_round_trip() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path();
        let claude_md = workspace.join("CLAUDE.md");
        fs::write(&claude_md, "# Old\n\nstuff\n").unwrap();
        let mut cfg = UnifiedConfig::default();
        let rule = workspace.join("rule.md");
        fs::write(&rule, "Use kebab-case.\n").unwrap();
        cfg.rules.files.push(rule);

        let out = render_memory(workspace, &cfg).unwrap();
        assert!(out.contains("Use kebab-case"));
        assert!(out.contains("# Old"));
    }
}
