//! `dh config show / validate / apply / restore` — unified-config layer entry
//! points. Lives in its own module to keep `commands/config.rs` focused on
//! the legacy KV/refresh commands.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use dh_config::{
    expand_config, load_layered, validate, ExpandContext, LoadOptions, NoopKeyringResolver,
    UnifiedConfig,
};
use dh_config_adapter::{
    apply, default_registry, AdapterRegistry, ApplyOptions, ConfigScope, FileChange,
};

// ───── User-visible defaults ─────

const TARGET_ALIAS_ALL: &str = "all";

#[derive(Subcommand, Debug)]
pub enum ConfigLayerCommands {
    /// Print the merged unified configuration to stdout.
    Show(ShowArgs),

    /// Validate the merged configuration without writing anything.
    Validate(ShowArgs),

    /// Render the unified config into one or more agent-native config files.
    ///
    /// `target` is an adapter key (e.g. `claudecode`, `opencode`) or the
    /// special value `all` to apply every registered adapter.
    Apply(ApplyArgs),
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Profile name to load (overrides the default declared in global config).
    #[arg(long)]
    pub profile: Option<String>,

    /// Workspace root used to discover the project layer; defaults to CWD.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct ApplyArgs {
    /// Adapter key (e.g. `claudecode`) or `all` to fan out.
    pub target: String,

    /// Apply at the given scope: `global` (default) or `project`.
    #[arg(long, default_value = "global")]
    pub scope: ScopeArg,

    /// Show the diff without writing anything.
    #[arg(long)]
    pub dry_run: bool,

    #[command(flatten)]
    pub load: ShowArgs,
}

#[derive(Clone, Debug, clap::ValueEnum)]
pub enum ScopeArg {
    Global,
    Project,
}

pub async fn run(cmd: ConfigLayerCommands) -> Result<(), anyhow::Error> {
    match cmd {
        ConfigLayerCommands::Show(args) => show(&args),
        ConfigLayerCommands::Validate(args) => validate_cmd(&args),
        ConfigLayerCommands::Apply(args) => apply_cmd(&args),
    }
}

// ───── show ─────

fn show(args: &ShowArgs) -> Result<(), anyhow::Error> {
    let loaded = load_layered(&load_options_from(args))?;
    println!("# sources:");
    for src in &loaded.sources {
        println!("#   {}", src.display());
    }
    if let Some(profile) = &loaded.profile {
        println!("# profile: {profile}");
    }
    let toml = toml::to_string_pretty(&loaded.config)
        .map_err(|e| anyhow::anyhow!("failed to format config: {e}"))?;
    print!("{toml}");
    Ok(())
}

// ───── validate ─────

fn validate_cmd(args: &ShowArgs) -> Result<(), anyhow::Error> {
    let loaded = load_layered(&load_options_from(args))?;
    let report = validate(&loaded.config);
    print_validation_report(&report);
    if !report.is_ok() {
        anyhow::bail!("configuration is invalid");
    }
    println!("OK");
    Ok(())
}

fn print_validation_report(report: &dh_config::ValidationReport) {
    for warning in report.warnings() {
        println!("warning: {warning}");
    }
    for err in report.errors() {
        println!("error:   {err}");
    }
}

// ───── apply ─────

fn apply_cmd(args: &ApplyArgs) -> Result<(), anyhow::Error> {
    let loaded = load_layered(&load_options_from(&args.load))?;
    validate(&loaded.config).into_result()?;

    let registry = default_registry();
    let scope = resolve_scope(&args.scope, &args.load.workspace)?;
    let cfg = expand_for_scope(loaded.config, &scope, args.dry_run)?;
    let opts = ApplyOptions {
        dry_run: args.dry_run,
        backup_dir: None,
    };
    let targets = expand_targets(&registry, &args.target)?;
    for adapter_key in targets {
        let outcome = apply(&registry, adapter_key, &cfg, &scope, &opts)?;
        print_outcome(&outcome, args.dry_run);
    }
    Ok(())
}

/// Expands placeholders inside the configuration, picking the workspace from
/// the chosen scope. Dry-runs use lenient mode so that missing env / keyring
/// values do not block previewing.
fn expand_for_scope(
    mut cfg: UnifiedConfig,
    scope: &ConfigScope,
    dry_run: bool,
) -> Result<UnifiedConfig, anyhow::Error> {
    let kr = NoopKeyringResolver;
    let workspace = scope.workspace().map(|p| p.to_path_buf());
    let mut ctx = ExpandContext::new(&kr);
    if let Some(ws) = workspace.as_ref() {
        ctx = ctx.with_workspace(ws);
    }
    if dry_run {
        ctx = ctx.lenient();
    }
    expand_config(&mut cfg, &ctx)?;
    Ok(cfg)
}
fn expand_targets<'a>(
    registry: &'a AdapterRegistry,
    target: &str,
) -> Result<Vec<&'a str>, anyhow::Error> {
    if target == TARGET_ALIAS_ALL {
        return Ok(registry.keys());
    }
    if registry.get(target).is_none() {
        anyhow::bail!(
            "unknown adapter `{target}`; available: {}",
            registry.keys().join(", ")
        );
    }
    Ok(vec![match registry.keys().into_iter().find(|k| *k == target) {
        Some(k) => k,
        None => unreachable!(),
    }])
}

fn print_outcome(outcome: &dh_config_adapter::ApplyOutcome, dry_run: bool) {
    let prefix = if dry_run { "[dry-run]" } else { "[applied]" };
    println!("{prefix} {}", outcome.adapter);
    for diff in &outcome.diffs {
        let label = match diff.change {
            FileChange::Created => "+",
            FileChange::Modified { .. } => "~",
            FileChange::Unchanged => "=",
        };
        println!("  {label} {}", diff.path.display());
    }
    if let Some(id) = &outcome.backup_id {
        println!("  backup: {}", id.as_str());
    }
}

// ───── helpers ─────

fn load_options_from(args: &ShowArgs) -> LoadOptions {
    LoadOptions {
        profile: args.profile.clone(),
        workspace: args
            .workspace
            .clone()
            .or_else(|| std::env::current_dir().ok()),
    }
}

fn resolve_scope(
    arg: &ScopeArg,
    workspace_override: &Option<PathBuf>,
) -> Result<ConfigScope, anyhow::Error> {
    match arg {
        ScopeArg::Global => Ok(ConfigScope::Global),
        ScopeArg::Project => {
            let workspace = workspace_override
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .ok_or_else(|| anyhow::anyhow!("could not determine workspace"))?;
            Ok(ConfigScope::Project(workspace))
        }
    }
}
