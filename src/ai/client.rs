//! Cliente delgado sobre la Messages API de Anthropic con tool-calling.
//! Mismo patron que `whatsapp/client.rs`: reqwest + tipos serde, sin logica
//! de negocio. El loop del agente (`src/ai/agent.rs`) es quien decide que
//! tools ofrecer y como interpretar las respuestas.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_MODEL: &str = "claude-sonnet-4-5";
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 1024;
// Sin timeout, una llamada colgada a Anthropic retiene el lock de esa
// conversacion indefinidamente y el caso queda congelado para cliente y
// asesor. 60s cubre el peor caso razonable de un turno con tools.
const REQUEST_TIMEOUT_SECS: u64 = 60;
const CONNECT_TIMEOUT_SECS: u64 = 10;

#[derive(Debug, Clone)]
pub struct AnthropicClient {
    http: reqwest::Client,
    api_key: String,
    model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    kind: &'static str,
}

impl CacheControl {
    const fn ephemeral() -> Self {
        Self { kind: "ephemeral" }
    }
}

/// Bloque de `system` como texto. El campo `cache_control` solo se serializa
/// en el bloque estatico: marca el punto de corte del prefijo cacheable
/// (tools + system estatico). El bloque dinamico (estado del caso, cambia
/// cada turno) va despues, sin marca, para no invalidar el cache en cada
/// llamada.
#[derive(Debug, Clone, Serialize)]
struct SystemBlock<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_control: Option<CacheControl>,
}

#[derive(Debug, Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: Vec<SystemBlock<'a>>,
    messages: &'a [Message],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    tools: &'a [ToolDefinition],
}

#[derive(Debug, Default, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

#[derive(Debug, Deserialize)]
pub struct MessagesResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<String>,
    #[serde(default)]
    pub usage: Usage,
}

impl AnthropicClient {
    pub fn new(api_key: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            http,
            api_key,
            model: DEFAULT_MODEL.to_string(),
        }
    }

    /// `static_system` es el bloque fijo (system prompt + implicitamente los
    /// tools, que van antes en el render de la API) y lleva el breakpoint de
    /// cache. `dynamic_system` es el bloque que cambia cada turno (estado del
    /// caso) y va sin `cache_control`, despues del estatico.
    pub async fn send_message(
        &self,
        static_system: &str,
        dynamic_system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<MessagesResponse, reqwest::Error> {
        let system = vec![
            SystemBlock {
                kind: "text",
                text: static_system,
                cache_control: Some(CacheControl::ephemeral()),
            },
            SystemBlock {
                kind: "text",
                text: dynamic_system,
                cache_control: None,
            },
        ];
        let request = MessagesRequest {
            model: &self.model,
            max_tokens: DEFAULT_MAX_TOKENS,
            system,
            messages,
            tools,
        };

        let response = self
            .http
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<MessagesResponse>()
            .await?;

        tracing::debug!(
            input_tokens = response.usage.input_tokens,
            output_tokens = response.usage.output_tokens,
            cache_creation_input_tokens = response.usage.cache_creation_input_tokens,
            cache_read_input_tokens = response.usage.cache_read_input_tokens,
            "anthropic messages usage"
        );

        Ok(response)
    }
}
