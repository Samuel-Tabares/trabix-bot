//! Cliente delgado sobre la Messages API de Anthropic con tool-calling.
//! Mismo patron que `whatsapp/client.rs`: reqwest + tipos serde, sin logica
//! de negocio. El loop del agente (`src/ai/agent.rs`) es quien decide que
//! tools ofrecer y como interpretar las respuestas.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_MODEL: &str = "claude-sonnet-5";
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
// Sonnet 5 corre thinking adaptativo, y esos tokens cuentan contra `max_tokens`.
// Con el tope de 1024 que traiamos de Sonnet 4.5 una respuesta larga se cortaba
// a la mitad. 4096 deja aire para pensar y responder; el cobro es por tokens
// realmente generados, no por el tope, asi que subirlo no cuesta por si solo.
const DEFAULT_MAX_TOKENS: u32 = 4096;
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
    // Con `thinking: adaptive` (ver `Thinking` mas abajo) el modelo decide
    // caso por caso si expone razonamiento visible; cuando lo hace, la
    // respuesta trae uno de estos dos bloques ANTES del texto/tool_use.
    // Sin estas variantes, `.json::<MessagesResponse>()` fallaba con
    // "unknown variant `thinking`" cada vez que el modelo pensaba en voz
    // alta, tumbando el turno completo a partir de v1.25.0 (incidente
    // 2026-09-10, cliente ...8927: "error decoding response body").
    // `signature`/`data` se re-serializan tal cual al meter el historial de
    // vuelta en el siguiente turno -- la API los valida para continuar un
    // razonamiento con tool use.
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: String,
    },
    RedactedThinking {
        data: String,
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

/// Thinking adaptativo: el modelo decide solo cuanto razonar antes de
/// responder. En Sonnet 5 es el unico modo "encendido" (el `budget_tokens` de
/// los modelos viejos ya no existe). Se activa a proposito: los errores que
/// costaron plata en produccion fueron de razonamiento, no de falta de datos —
/// el modelo tenia "Total de unidades en el pedido: 40" delante y hablo de 20.
#[derive(Debug, Serialize)]
struct Thinking {
    #[serde(rename = "type")]
    kind: &'static str,
}

/// `effort: low` mantiene el gasto de thinking corto y consolida las tool-calls.
/// Es lo adecuado para atencion al cliente por chat: turnos cortos, decisiones
/// simples. Los niveles altos se pagan en tokens y solo rinden en tareas largas
/// de razonamiento, que no es lo que hace este bot.
#[derive(Debug, Serialize)]
struct OutputConfig {
    effort: &'static str,
}

#[derive(Debug, Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: Vec<SystemBlock<'a>>,
    messages: &'a [Message],
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    tools: &'a [ToolDefinition],
    thinking: Thinking,
    output_config: OutputConfig,
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
            thinking: Thinking { kind: "adaptive" },
            output_config: OutputConfig { effort: "low" },
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

        // A nivel `info` a proposito: el filtro por defecto en produccion es
        // `granizado_bot=info`, asi que en `debug` esta linea nunca se vio y el
        // costo real por turno no era medible desde los logs de Railway.
        tracing::info!(
            input_tokens = response.usage.input_tokens,
            output_tokens = response.usage.output_tokens,
            cache_creation_input_tokens = response.usage.cache_creation_input_tokens,
            cache_read_input_tokens = response.usage.cache_read_input_tokens,
            "anthropic messages usage"
        );

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reproduce el incidente 2026-09-10 (cliente ...8927): con `thinking:
    // adaptive`, la API antepone un bloque `thinking` (o `redacted_thinking`)
    // al `text`/`tool_use` cuando el modelo decide exponer razonamiento.
    // Antes de estas variantes, esto tumbaba `.json::<MessagesResponse>()`
    // con "unknown variant `thinking`" y degradaba el turno completo.
    #[test]
    fn deserializes_response_with_visible_thinking_block() {
        let body = serde_json::json!({
            "content": [
                {"type": "thinking", "thinking": "el cliente pide 50 a Puerto Lopez", "signature": "abc123"},
                {"type": "text", "text": "Dame un momento para confirmar el envio."}
            ],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}
        });

        let response: MessagesResponse = serde_json::from_value(body).expect("should deserialize");
        assert!(matches!(response.content[0], ContentBlock::Thinking { .. }));
        assert!(matches!(response.content[1], ContentBlock::Text { .. }));
    }

    #[test]
    fn deserializes_response_with_redacted_thinking_block() {
        let body = serde_json::json!({
            "content": [
                {"type": "redacted_thinking", "data": "opaque"},
                {"type": "text", "text": "Listo."}
            ],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 5, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}
        });

        let response: MessagesResponse = serde_json::from_value(body).expect("should deserialize");
        assert!(matches!(response.content[0], ContentBlock::RedactedThinking { .. }));
    }

    #[test]
    fn round_trips_thinking_block_for_history_replay() {
        let original: ContentBlock = serde_json::from_value(serde_json::json!({
            "type": "thinking",
            "thinking": "razonamiento",
            "signature": "sig-1"
        }))
        .unwrap();

        let replayed = serde_json::to_value(&original).unwrap();
        assert_eq!(replayed["type"], "thinking");
        assert_eq!(replayed["signature"], "sig-1");
    }
}
