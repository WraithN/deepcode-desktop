use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedResponse {
    pub id: String,
    pub session_id: String,
    pub model: String,
    pub content: String,
    pub usage: TokenUsage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub session_id: String,
    pub delta: String,
    pub finish_reason: Option<String>,
}

/// 模型 Token 预估配置
#[derive(Debug, Clone)]
pub struct ModelTokenProfile {
    pub chars_per_token_chinese: f32,
    pub chars_per_token_other: f32,
    pub overhead_tokens: u32,
}

/// 内置模型预估配置表
pub fn resolve_model_profile(model: &str) -> ModelTokenProfile {
    const PROFILES: &[(&str, ModelTokenProfile)] = &[
        ("gpt-4o", ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
        ("gpt-4o-mini", ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
        ("gpt-4-turbo", ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
        ("gpt-4", ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
        ("gpt-3.5-turbo", ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }),
        ("claude-3-5-sonnet", ModelTokenProfile { chars_per_token_chinese: 1.3, chars_per_token_other: 3.5, overhead_tokens: 5 }),
        ("claude-3-opus", ModelTokenProfile { chars_per_token_chinese: 1.3, chars_per_token_other: 3.5, overhead_tokens: 5 }),
        ("claude-3-haiku", ModelTokenProfile { chars_per_token_chinese: 1.3, chars_per_token_other: 3.5, overhead_tokens: 5 }),
        ("deepseek-chat", ModelTokenProfile { chars_per_token_chinese: 1.4, chars_per_token_other: 3.8, overhead_tokens: 4 }),
        ("deepseek-coder", ModelTokenProfile { chars_per_token_chinese: 1.4, chars_per_token_other: 3.8, overhead_tokens: 4 }),
    ];

    for (key, profile) in PROFILES {
        if model == *key || model.starts_with(key) {
            return profile.clone();
        }
    }

    ModelTokenProfile { chars_per_token_chinese: 1.5, chars_per_token_other: 4.0, overhead_tokens: 3 }
}

/// 按模型预估 Token 数
pub fn estimate_tokens(payload: &str, model: &str) -> u32 {
    let profile = resolve_model_profile(model);
    let chinese_chars = payload.chars().filter(|&c| {
        matches!(c as u32,
            0x4E00..=0x9FFF |   // CJK Unified Ideographs
            0x3400..=0x4DBF |   // CJK Extension A
            0xF900..=0xFAFF |   // CJK Compatibility Ideographs
            0x3040..=0x309F |   // Hiragana
            0x30A0..=0x30FF |   // Katakana
            0xAC00..=0xD7AF     // Hangul Syllables
        )
    }).count();
    let other_chars = payload.chars().count() - chinese_chars;

    (chinese_chars as f32 / profile.chars_per_token_chinese
        + other_chars as f32 / profile.chars_per_token_other)
        .ceil() as u32
        + profile.overhead_tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_english() {
        let text = "Hello world this is a test message for token estimation.";
        let tokens = estimate_tokens(text, "gpt-4o");
        assert!(tokens >= 10 && tokens <= 25, "Expected ~17 tokens, got {}", tokens);
    }

    #[test]
    fn test_estimate_tokens_chinese() {
        let text = "这是一个中文测试消息，用于验证Token估算功能。";
        let tokens = estimate_tokens(text, "gpt-4o");
        assert!(tokens >= 15 && tokens <= 30, "Expected ~20 tokens, got {}", tokens);
    }

    #[test]
    fn test_resolve_model_profile_exact() {
        let p = resolve_model_profile("gpt-4o");
        assert_eq!(p.chars_per_token_chinese, 1.5);
        assert_eq!(p.overhead_tokens, 3);
    }

    #[test]
    fn test_resolve_model_profile_prefix() {
        let p = resolve_model_profile("gpt-4o-2024-08-06");
        assert_eq!(p.chars_per_token_chinese, 1.5);
    }

    #[test]
    fn test_resolve_model_profile_fallback() {
        let p = resolve_model_profile("unknown-model");
        assert_eq!(p.chars_per_token_chinese, 1.5);
        assert_eq!(p.overhead_tokens, 3);
    }

    #[test]
    fn test_claude_profile() {
        let p = resolve_model_profile("claude-3-5-sonnet");
        assert_eq!(p.chars_per_token_chinese, 1.3);
        assert_eq!(p.overhead_tokens, 5);
    }
}
