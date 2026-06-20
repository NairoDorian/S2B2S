// Adapted from AivoRelay (MaxITService/AIVORelay), MIT License.
// Source: src-tauri/src/url_security.rs — Canonical Base URLs (2026-06-19).

use crate::settings::{PostProcessProvider, APPLE_INTELLIGENCE_PROVIDER_ID};
use reqwest::Url;

pub const LLM_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const LLM_ZAI_BASE_URL: &str = "https://api.z.ai/api/paas/v4";
pub const LLM_GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
pub const LLM_GOOGLE_AI_STUDIO_BASE_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/openai/";
pub const LLM_OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const LLM_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";
pub const LLM_GROQ_BASE_URL: &str = "https://api.groq.com/openai/v1";
pub const LLM_CEREBRAS_BASE_URL: &str = "https://api.cerebras.ai/v1";
pub const LLM_BEDROCK_MANTLE_BASE_URL: &str = "https://bedrock-mantle.us-east-1.api.aws/v1";

fn parse_network_url(input: &str, context: &str) -> Result<Url, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(format!("{} is empty.", context));
    }
    Url::parse(trimmed).map_err(|err| format!("{} is invalid: {}", context, err))
}

fn normalize_url(url: &Url) -> String {
    url.as_str().trim_end_matches('/').to_string()
}

fn validate_network_base_url(
    input: &str,
    allow_insecure_http: bool,
    context: &str,
) -> Result<String, String> {
    let url = parse_network_url(input, context)?;

    match url.scheme() {
        "https" => Ok(normalize_url(&url)),
        "http" if allow_insecure_http => Ok(normalize_url(&url)),
        "http" => Err(format!(
            "{} must use HTTPS. Plain HTTP is allowed only for a Custom endpoint after enabling the advanced insecure HTTP override.",
            context
        )),
        scheme => Err(format!(
            "{} must use http:// or https://, but got '{}://'.",
            context, scheme
        )),
    }
}

pub fn is_plain_http_url(input: &str) -> bool {
    parse_network_url(input, "URL")
        .map(|url| url.scheme() == "http")
        .unwrap_or(false)
}

pub fn canonical_llm_provider_base_url(provider: &PostProcessProvider) -> Result<String, String> {
    match provider.id.as_str() {
        "openai" => Ok(LLM_OPENAI_BASE_URL.to_string()),
        "zai" => Ok(LLM_ZAI_BASE_URL.to_string()),
        "gemini" => Ok(LLM_GEMINI_BASE_URL.to_string()),
        "google_ai_studio" => Ok(LLM_GOOGLE_AI_STUDIO_BASE_URL.to_string()),
        "openrouter" => Ok(LLM_OPENROUTER_BASE_URL.to_string()),
        "anthropic" => Ok(LLM_ANTHROPIC_BASE_URL.to_string()),
        "groq" => Ok(LLM_GROQ_BASE_URL.to_string()),
        "cerebras" => Ok(LLM_CEREBRAS_BASE_URL.to_string()),
        "bedrock_mantle" => Ok(LLM_BEDROCK_MANTLE_BASE_URL.to_string()),
        "llama_cpp" => {
            // Llama.cpp is a local provider, allow HTTP
            validate_network_base_url(&provider.base_url, true, "Llama.cpp base URL")
        }
        "custom" => validate_network_base_url(
            &provider.base_url,
            provider.allow_insecure_http,
            "Custom LLM base URL",
        ),
        APPLE_INTELLIGENCE_PROVIDER_ID => Ok(provider.base_url.clone()),
        _ => validate_network_base_url(&provider.base_url, false, "LLM provider base URL"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(id: &str, base_url: &str, allow_insecure_http: bool) -> PostProcessProvider {
        PostProcessProvider {
            id: id.to_string(),
            label: id.to_string(),
            base_url: base_url.to_string(),
            allow_base_url_edit: true,
            allow_insecure_http,
            models_endpoint: None,
            supports_structured_output: false,
        }
    }

    #[test]
    fn is_plain_http_url_detects_plain_http_only() {
        assert!(is_plain_http_url(" http://localhost:8080/v1 "));
        assert!(!is_plain_http_url("https://localhost:8080/v1"));
        assert!(!is_plain_http_url("not-a-url"));
    }

    #[test]
    fn canonical_llm_provider_base_url_returns_known_provider_defaults() {
        assert_eq!(
            canonical_llm_provider_base_url(&provider(
                "openai",
                "https://ignored.example.com",
                false
            ))
            .unwrap(),
            LLM_OPENAI_BASE_URL
        );
        assert_eq!(
            canonical_llm_provider_base_url(&provider(
                "groq",
                "https://ignored.example.com",
                false
            ))
            .unwrap(),
            LLM_GROQ_BASE_URL
        );
        assert_eq!(
            canonical_llm_provider_base_url(&provider(
                "cerebras",
                "https://ignored.example.com",
                false
            ))
            .unwrap(),
            LLM_CEREBRAS_BASE_URL
        );
    }

    #[test]
    fn canonical_llm_provider_base_url_normalizes_custom_provider_urls() {
        let actual = canonical_llm_provider_base_url(&provider(
            "custom",
            " https://llm.example.com/v1/ ",
            false,
        ))
        .unwrap();
        assert_eq!(actual, "https://llm.example.com/v1");
    }

    #[test]
    fn canonical_llm_provider_base_url_rejects_custom_http_without_opt_in() {
        let error =
            canonical_llm_provider_base_url(&provider("custom", "http://llm.local/v1", false))
                .unwrap_err();
        assert!(error.contains("Custom LLM base URL"));
        assert!(error.contains("must use HTTPS"));
    }

    #[test]
    fn canonical_llm_provider_base_url_preserves_apple_intelligence_base_url() {
        let actual = canonical_llm_provider_base_url(&provider(
            APPLE_INTELLIGENCE_PROVIDER_ID,
            "apple-intelligence://on-device",
            false,
        ))
        .unwrap();
        assert_eq!(actual, "apple-intelligence://on-device");
    }

    #[test]
    fn canonical_llm_provider_base_url_validates_unknown_provider_as_https() {
        let error = canonical_llm_provider_base_url(&provider(
            "custom-like",
            "http://unsafe.example.com/v1",
            true,
        ))
        .unwrap_err();
        assert!(error.contains("LLM provider base URL"));
        assert!(error.contains("must use HTTPS"));
    }
}
