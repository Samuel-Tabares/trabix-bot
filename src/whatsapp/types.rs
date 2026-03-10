use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookPayload {
    pub entry: Vec<Entry>,
}

impl WebhookPayload {
    pub fn first_message(&self) -> Option<&IncomingMessage> {
        self.entry
            .iter()
            .flat_map(|entry| entry.changes.iter())
            .filter_map(|change| change.value.messages.as_ref())
            .flat_map(|messages| messages.iter())
            .next()
    }
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Contact {
    #[serde(default)]
    pub wa_id: Option<String>,
    #[serde(default)]
    pub profile: Option<ContactProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactProfile {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IncomingMessage {
    pub from: String,
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub text: Option<TextContent>,
    #[serde(default)]
    pub interactive: Option<InteractiveContent>,
    #[serde(default)]
    pub image: Option<ImageContent>,
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

#[cfg(test)]
mod tests {
    use super::{
        Button, ButtonAction, ButtonReplyPayload, Entry, InteractiveBody, InteractiveMessage,
        ListAction, ListReply, ListRow, ListSection, MarkAsRead, OutgoingButtonMessage,
        OutgoingImageBody, OutgoingImageMessage, OutgoingListMessage, OutgoingTextBody,
        OutgoingTextMessage, Value, WebhookPayload,
    };

    #[test]
    fn deserializes_text_message_payload() {
        let payload: WebhookPayload = serde_json::from_str(
            r#"{
                "entry": [{
                    "changes": [{
                        "value": {
                            "messages": [{
                                "from": "573001234567",
                                "type": "text",
                                "text": { "body": "Hola" },
                                "id": "wamid.xxx"
                            }]
                        }
                    }]
                }]
            }"#,
        )
        .expect("payload");

        let message = payload.first_message().expect("message");
        assert_eq!(message.kind, "text");
        assert_eq!(message.text.as_ref().expect("text").body, "Hola");
    }

    #[test]
    fn deserializes_button_reply_payload() {
        let payload: WebhookPayload = serde_json::from_str(
            r#"{
                "entry": [{
                    "changes": [{
                        "value": {
                            "messages": [{
                                "from": "573001234567",
                                "type": "interactive",
                                "interactive": {
                                    "type": "button_reply",
                                    "button_reply": {
                                        "id": "make_order",
                                        "title": "Hacer Pedido"
                                    }
                                },
                                "id": "wamid.xxx"
                            }]
                        }
                    }]
                }]
            }"#,
        )
        .expect("payload");

        let message = payload.first_message().expect("message");
        let reply = message
            .interactive
            .as_ref()
            .and_then(|interactive| interactive.button_reply.as_ref())
            .expect("button reply");

        assert_eq!(reply.id, "make_order");
        assert_eq!(reply.title, "Hacer Pedido");
    }

    #[test]
    fn deserializes_list_reply_payload() {
        let payload: WebhookPayload = serde_json::from_str(
            r#"{
                "entry": [{
                    "changes": [{
                        "value": {
                            "messages": [{
                                "from": "573001234567",
                                "type": "interactive",
                                "interactive": {
                                    "type": "list_reply",
                                    "list_reply": {
                                        "id": "view_menu",
                                        "title": "Ver Menú",
                                        "description": "Sabores y precios"
                                    }
                                },
                                "id": "wamid.xxx"
                            }]
                        }
                    }]
                }]
            }"#,
        )
        .expect("payload");

        let message = payload.first_message().expect("message");
        let reply: &ListReply = message
            .interactive
            .as_ref()
            .and_then(|interactive| interactive.list_reply.as_ref())
            .expect("list reply");

        assert_eq!(reply.id, "view_menu");
    }

    #[test]
    fn deserializes_image_payload() {
        let payload: WebhookPayload = serde_json::from_str(
            r#"{
                "entry": [{
                    "changes": [{
                        "value": {
                            "messages": [{
                                "from": "573001234567",
                                "type": "image",
                                "image": {
                                    "id": "media_id_xxx",
                                    "mime_type": "image/jpeg"
                                },
                                "id": "wamid.xxx"
                            }]
                        }
                    }]
                }]
            }"#,
        )
        .expect("payload");

        let message = payload.first_message().expect("message");
        assert_eq!(message.kind, "image");
        assert_eq!(message.image.as_ref().expect("image").id, "media_id_xxx");
    }

    #[test]
    fn serializes_outgoing_text_message() {
        let payload = OutgoingTextMessage {
            messaging_product: "whatsapp".into(),
            to: "573001234567".into(),
            kind: "text".into(),
            text: OutgoingTextBody {
                body: "Hola, bienvenido".into(),
            },
        };

        let value = serde_json::to_value(payload).expect("json");
        let expected = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": "573001234567",
            "type": "text",
            "text": { "body": "Hola, bienvenido" }
        });

        assert_eq!(value, expected);
    }

    #[test]
    fn serializes_outgoing_button_message() {
        let payload = OutgoingButtonMessage {
            messaging_product: "whatsapp".into(),
            to: "573001234567".into(),
            kind: "interactive".into(),
            interactive: InteractiveMessage {
                kind: "button".into(),
                body: InteractiveBody {
                    text: "¿Qué deseas hacer?".into(),
                },
                action: ButtonAction {
                    buttons: vec![
                        Button {
                            kind: "reply".into(),
                            reply: ButtonReplyPayload {
                                id: "make_order".into(),
                                title: "🧊 Hacer Pedido".into(),
                            },
                        },
                        Button {
                            kind: "reply".into(),
                            reply: ButtonReplyPayload {
                                id: "view_menu".into(),
                                title: "📋 Ver Menú".into(),
                            },
                        },
                        Button {
                            kind: "reply".into(),
                            reply: ButtonReplyPayload {
                                id: "view_schedule".into(),
                                title: "⏰ Horarios".into(),
                            },
                        },
                    ],
                },
            },
        };

        let value = serde_json::to_value(payload).expect("json");
        let expected = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": "573001234567",
            "type": "interactive",
            "interactive": {
                "type": "button",
                "body": { "text": "¿Qué deseas hacer?" },
                "action": {
                    "buttons": [
                        { "type": "reply", "reply": { "id": "make_order", "title": "🧊 Hacer Pedido" } },
                        { "type": "reply", "reply": { "id": "view_menu", "title": "📋 Ver Menú" } },
                        { "type": "reply", "reply": { "id": "view_schedule", "title": "⏰ Horarios" } }
                    ]
                }
            }
        });

        assert_eq!(value, expected);
    }

    #[test]
    fn serializes_outgoing_list_message() {
        let payload = OutgoingListMessage {
            messaging_product: "whatsapp".into(),
            to: "573001234567".into(),
            kind: "interactive".into(),
            interactive: InteractiveMessage {
                kind: "list".into(),
                body: InteractiveBody {
                    text: "¿Qué deseas hacer?".into(),
                },
                action: ListAction {
                    button: "Ver opciones".into(),
                    sections: vec![ListSection {
                        title: "Menú Principal".into(),
                        rows: vec![
                            ListRow {
                                id: "make_order".into(),
                                title: "🧊 Hacer Pedido".into(),
                                description: "Arma tu pedido de granizados".into(),
                            },
                            ListRow {
                                id: "view_menu".into(),
                                title: "📋 Ver Menú".into(),
                                description: "Sabores y precios".into(),
                            },
                            ListRow {
                                id: "view_schedule".into(),
                                title: "⏰ Horarios".into(),
                                description: "Horarios de entrega".into(),
                            },
                            ListRow {
                                id: "contact_advisor".into(),
                                title: "👨‍💼 Hablar con Asesor".into(),
                                description: "Contactar a un asesor".into(),
                            },
                        ],
                    }],
                },
            },
        };

        let value = serde_json::to_value(payload).expect("json");
        let expected = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": "573001234567",
            "type": "interactive",
            "interactive": {
                "type": "list",
                "body": { "text": "¿Qué deseas hacer?" },
                "action": {
                    "button": "Ver opciones",
                    "sections": [{
                        "title": "Menú Principal",
                        "rows": [
                            { "id": "make_order", "title": "🧊 Hacer Pedido", "description": "Arma tu pedido de granizados" },
                            { "id": "view_menu", "title": "📋 Ver Menú", "description": "Sabores y precios" },
                            { "id": "view_schedule", "title": "⏰ Horarios", "description": "Horarios de entrega" },
                            { "id": "contact_advisor", "title": "👨‍💼 Hablar con Asesor", "description": "Contactar a un asesor" }
                        ]
                    }]
                }
            }
        });

        assert_eq!(value, expected);
    }

    #[test]
    fn serializes_outgoing_image_message() {
        let payload = OutgoingImageMessage {
            messaging_product: "whatsapp".into(),
            to: "573001234567".into(),
            kind: "image".into(),
            image: OutgoingImageBody {
                id: "media_id_previamente_subido".into(),
                caption: Some("Menú de sabores con licor".into()),
            },
        };

        let value = serde_json::to_value(payload).expect("json");
        let expected = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": "573001234567",
            "type": "image",
            "image": {
                "id": "media_id_previamente_subido",
                "caption": "Menú de sabores con licor"
            }
        });

        assert_eq!(value, expected);
    }

    #[test]
    fn serializes_mark_as_read_payload() {
        let payload = MarkAsRead {
            messaging_product: "whatsapp".into(),
            status: "read".into(),
            message_id: "wamid.xxx".into(),
        };

        let value = serde_json::to_value(payload).expect("json");
        let expected = serde_json::json!({
            "messaging_product": "whatsapp",
            "status": "read",
            "message_id": "wamid.xxx"
        });

        assert_eq!(value, expected);
    }

    #[test]
    fn first_message_skips_status_only_events() {
        let payload = WebhookPayload {
            entry: vec![Entry {
                changes: vec![super::Change {
                    value: Value {
                        messages: None,
                        contacts: None,
                    },
                }],
            }],
        };

        assert!(payload.first_message().is_none());
    }
}
