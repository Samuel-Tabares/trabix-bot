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
    use super::{Change, Entry, IncomingMessage, TextContent, Value, WebhookPayload};

    fn text_message(id: &str, from: &str, body: &str) -> IncomingMessage {
        IncomingMessage {
            from: from.to_string(),
            id: id.to_string(),
            kind: "text".to_string(),
            text: Some(TextContent {
                body: body.to_string(),
            }),
            interactive: None,
            image: None,
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
                        },
                    }],
                },
                Entry {
                    changes: vec![Change {
                        value: Value {
                            messages: Some(vec![text_message("wamid-3", "573002222222", "menu")]),
                            contacts: None,
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
}
