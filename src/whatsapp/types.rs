use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookPayload {
    pub entry: Vec<Entry>,
}

impl WebhookPayload {
    pub fn messages(&self) -> impl Iterator<Item = &IncomingMessage> {
        self.entry
            .iter()
            .flat_map(|entry| entry.changes.iter())
            .filter_map(|change| change.value.messages.as_ref())
            .flat_map(|messages| messages.iter())
    }

    pub fn message_events(&self) -> Vec<IncomingMessageEvent> {
        let mut events = Vec::new();

        for entry in &self.entry {
            for change in &entry.changes {
                let Some(messages) = change.value.messages.as_ref() else {
                    continue;
                };

                let contacts = change.value.contacts.as_deref().unwrap_or(&[]);

                for (index, message) in messages.iter().enumerate() {
                    let contact = contacts
                        .iter()
                        .find(|contact| contact.wa_id.as_deref() == Some(message.from.as_str()))
                        .cloned()
                        .or_else(|| {
                            if contacts.len() == 1 && messages.len() == 1 {
                                contacts.first().cloned()
                            } else if contacts.len() == messages.len() {
                                contacts.get(index).cloned()
                            } else {
                                None
                            }
                        });

                    events.push(IncomingMessageEvent {
                        message: message.clone(),
                        contact,
                    });
                }
            }
        }

        events
    }

    pub fn status_events(&self) -> Vec<StatusEvent> {
        self.entry
            .iter()
            .flat_map(|entry| entry.changes.iter())
            .flat_map(|change| change.value.statuses.iter().flatten())
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IncomingMessageEvent {
    pub message: IncomingMessage,
    #[serde(default)]
    pub contact: Option<Contact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Change {
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Value {
    #[serde(default)]
    pub messages: Option<Vec<IncomingMessage>>,
    #[serde(default)]
    pub contacts: Option<Vec<Contact>>,
    #[serde(default)]
    pub statuses: Option<Vec<StatusEvent>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusEvent {
    #[serde(default)]
    pub id: Option<String>,
    pub status: String,
    #[serde(default)]
    pub recipient_id: Option<String>,
    /// Solo presente cuando `status == "failed"` — Meta manda el motivo acá
    /// (p. ej. 131047, ventana de 24h cerrada). Ver `record_delivery_status`.
    #[serde(default)]
    pub errors: Vec<StatusError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusError {
    #[serde(default)]
    pub code: Option<i64>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Contact {
    #[serde(default)]
    pub wa_id: Option<String>,
    #[serde(default)]
    pub profile: Option<ContactProfile>,
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactProfile {
    /// Opcional a propósito: Meta manda `"profile": {}` sin `name` cuando el
    /// usuario no tiene nombre de perfil configurado. Con este campo obligatorio
    /// serde reventaba el payload ENTERO y el mensaje del cliente se perdía sin
    /// respuesta — el bot ni se enteraba de que había escrito.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IncomingMessage {
    pub from: String,
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub context: Option<MessageContext>,
    #[serde(default)]
    pub text: Option<TextContent>,
    #[serde(default)]
    pub interactive: Option<InteractiveContent>,
    #[serde(default)]
    pub image: Option<ImageContent>,
    #[serde(default)]
    pub referral: Option<MessageReferral>,
}

/// Presente solo en el primer mensaje de un cliente que llegó por un anuncio
/// click-to-WhatsApp (CTWA). `ctwa_clid` es lo que hay que reenviar a la
/// Conversions API de Meta junto con la compra para cerrar el lazo de
/// atribución (ver docs/PENDIENTE_capi_meta.md).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageReferral {
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub ctwa_clid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageContext {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextContent {
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractiveContent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub button_reply: Option<ButtonReply>,
    #[serde(default)]
    pub list_reply: Option<ListReply>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ButtonReply {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListReply {
    pub id: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageContent {
    pub id: String,
    #[serde(default)]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutgoingTextMessage {
    pub messaging_product: String,
    pub to: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub text: OutgoingTextBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutgoingTextBody {
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutgoingButtonMessage {
    pub messaging_product: String,
    pub to: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub interactive: InteractiveMessage<ButtonAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutgoingListMessage {
    pub messaging_product: String,
    pub to: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub interactive: InteractiveMessage<ListAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractiveMessage<T> {
    #[serde(rename = "type")]
    pub kind: String,
    pub body: InteractiveBody,
    pub action: T,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InteractiveBody {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ButtonAction {
    pub buttons: Vec<Button>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Button {
    #[serde(rename = "type")]
    pub kind: String,
    pub reply: ButtonReplyPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ButtonReplyPayload {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListAction {
    pub button: String,
    pub sections: Vec<ListSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListSection {
    pub title: String,
    pub rows: Vec<ListRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListRow {
    pub id: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutgoingImageMessage {
    pub messaging_product: String,
    pub to: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub image: OutgoingImageBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutgoingImageBody {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarkAsRead {
    pub messaging_product: String,
    pub status: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MessageSendResponse {
    #[serde(default)]
    pub messages: Vec<MessageSendId>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MessageSendId {
    pub id: String,
}

#[cfg(test)]
mod tests {
    use super::{
        Change, Contact, ContactProfile, Entry, IncomingMessage, StatusEvent, TextContent, Value,
        WebhookPayload,
    };

    fn text_message(id: &str, from: &str, body: &str) -> IncomingMessage {
        IncomingMessage {
            from: from.to_string(),
            id: id.to_string(),
            kind: "text".to_string(),
            context: None,
            text: Some(TextContent {
                body: body.to_string(),
            }),
            interactive: None,
            image: None,
            referral: None,
        }
    }

    #[test]
    fn webhook_payload_iterates_all_messages_in_order() {
        let payload = WebhookPayload {
            entry: vec![
                Entry {
                    changes: vec![Change {
                        value: Value {
                            messages: Some(vec![
                                text_message("wamid-1", "573001111111", "hola"),
                                text_message("wamid-2", "573001111111", "quiero pedir"),
                            ]),
                            contacts: None,
                            statuses: None,
                        },
                    }],
                },
                Entry {
                    changes: vec![Change {
                        value: Value {
                            messages: Some(vec![text_message("wamid-3", "573002222222", "menu")]),
                            contacts: None,
                            statuses: None,
                        },
                    }],
                },
            ],
        };

        let message_ids = payload
            .messages()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(message_ids, vec!["wamid-1", "wamid-2", "wamid-3"]);
    }

    #[test]
    fn webhook_payload_matches_contacts_by_wa_id() {
        let payload = WebhookPayload {
            entry: vec![Entry {
                changes: vec![Change {
                    value: Value {
                        messages: Some(vec![text_message("wamid-1", "573001111111", "hola")]),
                        contacts: Some(vec![Contact {
                            wa_id: Some("573001111111".to_string()),
                            profile: Some(ContactProfile {
                                name: Some("Ana Maria".to_string()),
                            }),
                            username: None,
                        }]),
                        statuses: None,
                    },
                }],
            }],
        };

        let events = payload.message_events();

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0]
                .contact
                .as_ref()
                .and_then(|contact| contact.profile.as_ref())
                .and_then(|profile| profile.name.as_deref()),
            Some("Ana Maria")
        );
    }

    #[test]
    fn webhook_payload_uses_positional_fallback_for_single_message_single_contact() {
        let payload = WebhookPayload {
            entry: vec![Entry {
                changes: vec![Change {
                    value: Value {
                        messages: Some(vec![text_message("wamid-1", "573001111111", "hola")]),
                        contacts: Some(vec![Contact {
                            wa_id: Some("573009999999".to_string()),
                            profile: Some(ContactProfile {
                                name: Some("Ana Maria".to_string()),
                            }),
                            username: None,
                        }]),
                        statuses: None,
                    },
                }],
            }],
        };

        let events = payload.message_events();

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0]
                .contact
                .as_ref()
                .and_then(|contact| contact.profile.as_ref())
                .and_then(|profile| profile.name.as_deref()),
            Some("Ana Maria")
        );
    }

    #[test]
    fn webhook_payload_does_not_guess_when_contact_count_is_ambiguous() {
        let payload = WebhookPayload {
            entry: vec![Entry {
                changes: vec![Change {
                    value: Value {
                        messages: Some(vec![
                            text_message("wamid-1", "573001111111", "hola"),
                            text_message("wamid-2", "573002222222", "menu"),
                        ]),
                        contacts: Some(vec![Contact {
                            wa_id: Some("573003333333".to_string()),
                            profile: Some(ContactProfile {
                                name: Some("Ana Maria".to_string()),
                            }),
                            username: None,
                        }]),
                        statuses: None,
                    },
                }],
            }],
        };

        let events = payload.message_events();

        assert_eq!(events.len(), 2);
        assert!(events.iter().all(|event| event.contact.is_none()));
    }

    #[test]
    fn webhook_payload_collects_status_events() {
        let payload = WebhookPayload {
            entry: vec![Entry {
                changes: vec![Change {
                    value: Value {
                        messages: None,
                        contacts: None,
                        statuses: Some(vec![StatusEvent {
                            id: Some("wamid-1".to_string()),
                            status: "delivered".to_string(),
                            recipient_id: Some("573001111111".to_string()),
                            errors: Vec::new(),
                        }]),
                    },
                }],
            }],
        };

        let statuses = payload.status_events();

        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].status, "delivered");
        assert_eq!(statuses[0].recipient_id.as_deref(), Some("573001111111"));
    }

    #[test]
    fn parses_failed_status_with_error_details() {
        // Forma real del webhook de Meta cuando un envio fuera de la ventana
        // de 24h falla despues de haber sido aceptado por la API (motivo por
        // el que se agrego `errors` a StatusEvent, ver record_delivery_status).
        let raw = r#"
        {
          "entry": [{
            "changes": [{
              "value": {
                "statuses": [{
                  "id": "wamid-failed",
                  "status": "failed",
                  "recipient_id": "573001111111",
                  "errors": [{
                    "code": 131047,
                    "title": "Re-engagement message"
                  }]
                }]
              }
            }]
          }]
        }
        "#;

        let payload: WebhookPayload =
            serde_json::from_str(raw).expect("meta status payload should deserialize");
        let statuses = payload.status_events();

        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].status, "failed");
        assert_eq!(statuses[0].errors.len(), 1);
        assert_eq!(statuses[0].errors[0].code, Some(131047));
        assert_eq!(statuses[0].errors[0].title.as_deref(), Some("Re-engagement message"));
    }

    #[test]
    fn parses_external_customer_payload_with_profile_and_context_variants() {
        let raw = r#"
        {
          "entry": [{
            "changes": [{
              "value": {
                "contacts": [{
                  "profile": { "name": "Cliente Externo" },
                  "wa_id": "573001234567"
                }],
                "messages": [
                  {
                    "from": "573001234567",
                    "id": "wamid-text",
                    "timestamp": "1710000000",
                    "text": { "body": "hola" },
                    "type": "text"
                  },
                  {
                    "from": "573001234567",
                    "id": "wamid-button",
                    "timestamp": "1710000001",
                    "context": {},
                    "interactive": {
                      "type": "button_reply",
                      "button_reply": { "id": "main_order", "title": "Hacer Pedido" }
                    },
                    "type": "interactive"
                  },
                  {
                    "from": "573001234567",
                    "id": "wamid-list",
                    "timestamp": "1710000002",
                    "context": { "id": "" },
                    "interactive": {
                      "type": "list_reply",
                      "list_reply": {
                        "id": "flavor_maracumango",
                        "title": "Maracumango",
                        "description": ""
                      }
                    },
                    "type": "interactive"
                  }
                ]
              }
            }]
          }]
        }
        "#;

        let payload: WebhookPayload =
            serde_json::from_str(raw).expect("meta payload should deserialize");
        let events = payload.message_events();

        assert_eq!(events.len(), 3);
        assert!(events
            .iter()
            .all(|event| event.message.from == "573001234567"));
        assert!(events.iter().all(|event| event
            .contact
            .as_ref()
            .and_then(|contact| contact.profile.as_ref())
            .and_then(|profile| profile.name.as_deref())
            == Some("Cliente Externo")));
        assert!(events[0].message.context.is_none());
        assert_eq!(
            events[1]
                .message
                .context
                .as_ref()
                .and_then(|ctx| ctx.id.as_deref()),
            None
        );
        assert_eq!(
            events[2]
                .message
                .context
                .as_ref()
                .and_then(|ctx| ctx.id.as_deref()),
            Some("")
        );
    }

    /// Regresión de un incidente real (2026-08-02): Meta mandó `"profile": {}`
    /// sin `name` y, con el campo obligatorio, serde rechazaba el payload
    /// COMPLETO (`missing field 'name'`). El webhook devolvía error y el mensaje
    /// del cliente se perdía: nunca llegaba al motor, el cliente no recibía nada
    /// y no quedaba ni rastro en `message_events`.
    #[test]
    fn parses_contact_without_profile_name() {
        let raw = serde_json::json!({
          "entry": [{
            "changes": [{
              "value": {
                "contacts": [{
                  "profile": {},
                  "wa_id": "573001234567"
                }],
                "messages": [{
                  "from": "573001234567",
                  "id": "wamid-sin-nombre",
                  "type": "text",
                  "text": { "body": "Hola" }
                }]
              }
            }]
          }]
        });

        let payload: super::WebhookPayload =
            serde_json::from_value(raw).expect("un perfil sin nombre no puede tumbar el payload");
        let events = payload.message_events();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].message.from, "573001234567");
        assert!(events[0]
            .contact
            .as_ref()
            .and_then(|contact| contact.profile.as_ref())
            .and_then(|profile| profile.name.as_deref())
            .is_none());
    }
}
