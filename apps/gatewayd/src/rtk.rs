use dh_core::{Message, Role, UnifiedRequest};
use tracing::info;

pub struct RtkConfig {
    pub enabled: bool,
    pub max_context_messages: usize,
    pub summary_threshold: usize,
    pub compress_whitespace: bool,
    pub deduplicate_system: bool,
}

impl Default for RtkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_context_messages: 20,
            summary_threshold: 10,
            compress_whitespace: true,
            deduplicate_system: true,
        }
    }
}

pub struct RtkEngine {
    config: RtkConfig,
}

impl RtkEngine {
    pub fn new(config: RtkConfig) -> Self {
        Self { config }
    }

    pub fn default_engine() -> Self {
        Self::new(RtkConfig::default())
    }

    pub fn optimize(&self, req: &mut UnifiedRequest) {
        if !self.config.enabled {
            return;
        }

        if self.config.deduplicate_system {
            self.deduplicate_system_prompts(req);
        }

        if req.messages.len() > self.config.summary_threshold {
            self.apply_sliding_window(req);
        }

        if self.config.compress_whitespace {
            self.compress_prompts(req);
        }

        info!(
            "RTK optimized request: {} messages -> {} messages",
            req.messages.len(),
            req.messages.len()
        );
    }

    fn deduplicate_system_prompts(&self, req: &mut UnifiedRequest) {
        let mut seen_system = false;
        req.messages.retain(|m| {
            if matches!(m.role, Role::System) {
                if seen_system {
                    return false;
                }
                seen_system = true;
            }
            true
        });
    }

    fn apply_sliding_window(&self, req: &mut UnifiedRequest) {
        let total = req.messages.len();
        if total <= self.config.max_context_messages {
            return;
        }

        let keep_recent = self.config.max_context_messages / 2;
        let summarize_count = total - keep_recent;

        let mut summarized = Vec::new();
        let mut summary_parts = Vec::new();

        for (i, msg) in req.messages.iter().take(summarize_count).enumerate() {
            let prefix = match msg.role {
                Role::System => "SYS",
                Role::User => "USR",
                Role::Assistant => "AST",
                Role::Tool => "TL",
            };
            summary_parts.push(format!(
                "[{}:{}] {}",
                i,
                prefix,
                &msg.content[..msg.content.len().min(80)]
            ));
        }

        if !summary_parts.is_empty() {
            summarized.push(Message {
                role: Role::System,
                content: format!(
                    "Previous conversation summary ({} messages condensed): {}",
                    summarize_count,
                    summary_parts.join("; ")
                ),
            });
        }

        summarized.extend(req.messages.drain(summarize_count..));
        req.messages = summarized;

        info!(
            "RTK sliding window: {} -> {} messages (summarized {})",
            total,
            req.messages.len(),
            summarize_count
        );
    }

    fn compress_prompts(&self, req: &mut UnifiedRequest) {
        for msg in &mut req.messages {
            let original_len = msg.content.len();
            msg.content = msg
                .content
                .lines()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            msg.content = msg.content.replace("\n\n\n", "\n\n");
            if msg.content.len() < original_len {
                info!(
                    "RTK compressed message: {} -> {} bytes",
                    original_len,
                    msg.content.len()
                );
            }
        }
    }
}
