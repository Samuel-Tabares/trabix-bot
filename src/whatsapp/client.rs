use std::{error::Error, fmt};

use reqwest::{Client, StatusCode};

use super::{
    buttons::quick_buttons,
    types::{
        Button, InteractiveBody, InteractiveMessage, ListAction, ListSection, MarkAsRead,
        OutgoingImageBody, OutgoingImageMessage, OutgoingListMessage, OutgoingTextBody,
        OutgoingTextMessage,
    },
};

#[derive(Debug, Clone)]
pub struct WhatsAppClient {
    http_client: Client,
    whatsapp_token: String,
    whatsapp_phone_id: String,
}

#[derive(Debug)]
pub enum WhatsAppError {
    Request(reqwest::Error),
    Api { status: StatusCode, body: String },
}

impl fmt::Display for WhatsAppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(err) => write!(f, "whatsapp request failed: {err}"),
            Self::Api { status, body } => write!(f, "whatsapp api error {status}: {body}"),
        }
    }
}

impl Error for WhatsAppError {}

impl From<reqwest::Error> for WhatsAppError {
    fn from(value: reqwest::Error) -> Self {
        Self::Request(value)
    }
}

impl WhatsAppClient {
    pub fn new(whatsapp_token: String, whatsapp_phone_id: String) -> Self {
        Self {
            http_client: Client::new(),
            whatsapp_token,
            whatsapp_phone_id,
        }
    }

    pub async fn send_text(&self, to: &str, body: &str) -> Result<(), WhatsAppError> {
        let payload = OutgoingTextMessage {
            messaging_product: "whatsapp".into(),
            to: to.into(),
            kind: "text".into(),
            text: OutgoingTextBody { body: body.into() },
        };

        self.post_message("text", to, &payload).await
    }

    pub async fn send_buttons(
        &self,
        to: &str,
        body: &str,
        buttons: Vec<Button>,
    ) -> Result<(), WhatsAppError> {
        let payload = quick_buttons(
            to,
            body,
            &buttons
                .iter()
                .map(|button| (button.reply.id.as_str(), button.reply.title.as_str()))
                .collect::<Vec<_>>(),
        );

        self.post_message("button", to, &payload).await
    }

    pub async fn send_list(
        &self,
        to: &str,
        body: &str,
        button_text: &str,
        sections: Vec<ListSection>,
    ) -> Result<(), WhatsAppError> {
        let payload = OutgoingListMessage {
            messaging_product: "whatsapp".into(),
            to: to.into(),
            kind: "interactive".into(),
            interactive: InteractiveMessage {
                kind: "list".into(),
                body: InteractiveBody { text: body.into() },
                action: ListAction {
                    button: button_text.into(),
                    sections,
                },
            },
        };

        self.post_message("list", to, &payload).await
    }

    pub async fn send_image(
        &self,
        to: &str,
        media_id: &str,
        caption: Option<&str>,
    ) -> Result<(), WhatsAppError> {
        let payload = OutgoingImageMessage {
            messaging_product: "whatsapp".into(),
            to: to.into(),
            kind: "image".into(),
            image: OutgoingImageBody {
                id: media_id.into(),
                caption: caption.map(str::to_owned),
            },
        };

        self.post_message("image", to, &payload).await
    }

    pub async fn mark_as_read(&self, message_id: &str) -> Result<(), WhatsAppError> {
        let payload = MarkAsRead {
            messaging_product: "whatsapp".into(),
            status: "read".into(),
            message_id: message_id.into(),
        };

        self.post_message("read", "n/a", &payload).await
    }

    async fn post_message<T: serde::Serialize>(
        &self,
        message_type: &str,
        to: &str,
        payload: &T,
    ) -> Result<(), WhatsAppError> {
        let url = format!(
            "https://graph.facebook.com/v21.0/{}/messages",
            self.whatsapp_phone_id
        );
        let response = self
            .http_client
            .post(url)
            .bearer_auth(&self.whatsapp_token)
            .json(payload)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_else(|_| "<unable to read body>".into());
            tracing::error!(to = %to, message_type = %message_type, %status, body = %body, "meta api returned an error");
            return Err(WhatsAppError::Api { status, body });
        }

        tracing::info!(to = %to, message_type = %message_type, "sent whatsapp message");
        Ok(())
    }
}
