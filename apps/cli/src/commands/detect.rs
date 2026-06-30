use clap::Args;

/// Detect which coding agents are installed on the system.
#[derive(Args, Debug)]
pub struct DetectArgs {
    /// Output result as JSON
    #[arg(long)]
    pub json: bool,
}

/// Information about a supported coding agent.
struct AgentInfo {
    key: &'static str,
    name: &'static str,
    program: &'static str,
    version_flag: &'static str,
}

/// Result of detecting a single agent.
#[derive(Debug)]
pub struct AgentDetectionResult {
    pub key: String,
    pub name: String,
    pub installed: bool,
}

const SUPPORTED_AGENTS: &[AgentInfo] = &[
    AgentInfo {
        key: "opencode",
        name: "OpenCode",
        program: "opencode",
        version_flag: "--version",
    },
    AgentInfo {
        key: "claude-code",
        name: "Claude Code",
        program: "claude",
        version_flag: "--version",
    },
    AgentInfo {
        key: "codex",
        name: "Codex",
        program: "codex",
        version_flag: "--version",
    },
];

/// Checks whether a CLI program is installed by running it with a version flag.
fn is_command_installed(program: &str, version_flag: &str) -> bool {
    std::process::Command::new(program)
        .arg(version_flag)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Detect all supported coding agents and return their installation status.
pub fn detect_agents() -> Vec<AgentDetectionResult> {
    SUPPORTED_AGENTS
        .iter()
        .map(|agent| AgentDetectionResult {
            key: agent.key.to_string(),
            name: agent.name.to_string(),
            installed: is_command_installed(agent.program, agent.version_flag),
        })
        .collect()
}

/// Returns true if at least one supported coding agent is installed.
pub fn has_any_agent_installed() -> bool {
    detect_agents().iter().any(|agent| agent.installed)
}

/// Prints a hint telling the user to install a coding agent first.
pub fn print_missing_agent_hint() {
    println!("No supported coding agent detected on this system.");
    println!("Please install at least one of the following:");
    for agent in SUPPORTED_AGENTS {
        println!("  - {} ({}): {}", agent.name, agent.key, agent.program);
    }
}

/// Run the `dh detect` command.
pub fn run(args: DetectArgs) -> Result<(), anyhow::Error> {
    let results = detect_agents();

    if args.json {
        let json: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "key": r.key,
                    "name": r.name,
                    "installed": r.installed,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(());
    }

    println!("Detected coding agents:");
    let mut any_installed = false;
    for result in &results {
        let status = if result.installed {
            "✓ installed"
        } else {
            "✗ not installed"
        };
        println!("  {:<12} {}", result.name, status);
        if result.installed {
            any_installed = true;
        }
    }

    if !any_installed {
        println!();
        print_missing_agent_hint();
    }

    Ok(())
}
