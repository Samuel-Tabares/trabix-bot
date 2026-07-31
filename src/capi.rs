use reqwest::Client;
use serde::Serialize;

/// Cliente de la Conversions API de Meta para reportar compras confirmadas y
/// cerrar el lazo de atribución de los anuncios click-to-WhatsApp (ver
/// docs/PENDIENTE_capi_meta.md). Es telemetría, no ruta crítica: toda llamada
/// falla en silencio y solo loguea — nunca debe tumbar ni demorar la
/// confirmación de un pedido.
///
/// `None` en `dataset_id`/`access_token`/`waba_id` (no configurados aún)
/// convierte cada envío en un no-op silencioso.
#[derive(Debug, Clone)]
pub struct CapiClient {
    http_client: Client,
    dataset_id: Option<String>,
    access_token: Option<String>,
    waba_id: Option<String>,
}

#[derive(Serialize)]
struct CapiPayload {
    data: Vec<CapiEvent>,
}

#[derive(Serialize)]
struct CapiEvent {
    event_name: &'static str,
    event_time: i64,
    action_source: &'static str,
    messaging_channel: &'static str,
    event_id: String,
    user_data: CapiUserData,
    custom_data: CapiCustomData,
}

#[derive(Serialize)]
struct CapiUserData {
    whatsapp_business_account_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ctwa_clid: Option<String>,
}

#[derive(Serialize)]
struct CapiCustomData {
    currency: &'static str,
    value: i32,
}

impl CapiClient {
    pub fn new(
        dataset_id: Option<String>,
        access_token: Option<String>,
        waba_id: Option<String>,
    ) -> Self {
        Self {
            http_client: Client::new(),
            dataset_id,
            access_token,
            waba_id,
        }
    }

    /// Reporta una compra confirmada. `value_cop` es el valor de venta en
    /// pesos colombianos (sin decimales). No propaga errores: cualquier
    /// fallo (config incompleta, red, respuesta de error de Meta) se loguea
    /// y se descarta.
    pub async fn report_purchase(&self, order_id: i32, ctwa_clid: Option<String>, value_cop: i32) {
        let (Some(dataset_id), Some(access_token), Some(waba_id)) =
            (&self.dataset_id, &self.access_token, &self.waba_id)
        else {
            tracing::debug!(
                order_id,
                "CAPI no configurado (falta dataset id, token o WABA id): omitiendo evento Purchase"
            );
            return;
        };

        let event_id = format!("trabix-order-{order_id}-{}", chrono::Utc::now().timestamp_millis());
        let payload = CapiPayload {
            data: vec![CapiEvent {
                event_name: "Purchase",
                event_time: chrono::Utc::now().timestamp(),
                action_source: "business_messaging",
                messaging_channel: "whatsapp",
                event_id,
                user_data: CapiUserData {
                    whatsapp_business_account_id: waba_id.clone(),
                    ctwa_clid,
                },
                custom_data: CapiCustomData {
                    currency: "COP",
                    value: value_cop,
                },
            }],
        };

        let url = format!("https://graph.facebook.com/v21.0/{dataset_id}/events");
        let response = self
            .http_client
            .post(url)
            .query(&[("access_token", access_token.as_str())])
            .json(&payload)
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(order_id, value_cop, "reported purchase to meta CAPI");
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "<unable to read body>".into());
                tracing::warn!(order_id, %status, body = %body, "meta CAPI returned an error");
            }
            Err(err) => {
                tracing::warn!(order_id, error = %err, "failed to reach meta CAPI");
            }
        }
    }
}
