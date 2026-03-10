#![allow(dead_code)]

use super::types::{
    Button, ButtonAction, ButtonReplyPayload, InteractiveBody, InteractiveMessage, ListAction,
    ListRow, ListSection, OutgoingButtonMessage, OutgoingListMessage,
};

pub fn quick_buttons(to: &str, body: &str, buttons: &[(&str, &str)]) -> OutgoingButtonMessage {
    OutgoingButtonMessage {
        messaging_product: "whatsapp".into(),
        to: to.into(),
        kind: "interactive".into(),
        interactive: InteractiveMessage {
            kind: "button".into(),
            body: InteractiveBody { text: body.into() },
            action: ButtonAction {
                buttons: buttons
                    .iter()
                    .map(|(id, title)| Button {
                        kind: "reply".into(),
                        reply: ButtonReplyPayload {
                            id: (*id).into(),
                            title: (*title).into(),
                        },
                    })
                    .collect(),
            },
        },
    }
}

pub fn quick_list(
    to: &str,
    body: &str,
    button_text: &str,
    rows: &[(&str, &str, &str)],
) -> OutgoingListMessage {
    OutgoingListMessage {
        messaging_product: "whatsapp".into(),
        to: to.into(),
        kind: "interactive".into(),
        interactive: InteractiveMessage {
            kind: "list".into(),
            body: InteractiveBody { text: body.into() },
            action: ListAction {
                button: button_text.into(),
                sections: vec![ListSection {
                    title: "Opciones".into(),
                    rows: rows
                        .iter()
                        .map(|(id, title, description)| ListRow {
                            id: (*id).into(),
                            title: (*title).into(),
                            description: (*description).into(),
                        })
                        .collect(),
                }],
            },
        },
    }
}
