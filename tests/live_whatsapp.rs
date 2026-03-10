use granizado_bot::whatsapp::{
    client::WhatsAppClient,
    types::{Button, ButtonReplyPayload, ListRow, ListSection},
};

fn load_env() {
    let _ = dotenvy::dotenv();
}

fn required_env(name: &str) -> String {
    load_env();
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for live WhatsApp smoke tests"))
}

#[tokio::test]
#[ignore = "requires live Meta credentials and a tester recipient"]
async fn sends_text_buttons_and_list_messages() {
    let token = required_env("WHATSAPP_TOKEN");
    let phone_id = required_env("WHATSAPP_PHONE_ID");
    let recipient = std::env::var("WHATSAPP_TEST_RECIPIENT")
        .or_else(|_| std::env::var("ADVISOR_PHONE"))
        .expect("WHATSAPP_TEST_RECIPIENT or ADVISOR_PHONE must be set for live WhatsApp smoke tests");

    let client = WhatsAppClient::new(token, phone_id);

    client
        .send_text(&recipient, "Smoke test: texto Fase 1")
        .await
        .expect("send_text should succeed");

    client
        .send_buttons(
            &recipient,
            "Smoke test: botones Fase 1",
            vec![
                Button {
                    kind: "reply".into(),
                    reply: ButtonReplyPayload {
                        id: "smoke_one".into(),
                        title: "Opción 1".into(),
                    },
                },
                Button {
                    kind: "reply".into(),
                    reply: ButtonReplyPayload {
                        id: "smoke_two".into(),
                        title: "Opción 2".into(),
                    },
                },
                Button {
                    kind: "reply".into(),
                    reply: ButtonReplyPayload {
                        id: "smoke_three".into(),
                        title: "Opción 3".into(),
                    },
                },
            ],
        )
        .await
        .expect("send_buttons should succeed");

    client
        .send_list(
            &recipient,
            "Smoke test: lista Fase 1",
            "Ver opciones",
            vec![ListSection {
                title: "Smoke Test".into(),
                rows: vec![
                    ListRow {
                        id: "smoke_list_one".into(),
                        title: "Lista 1".into(),
                        description: "Primera opción".into(),
                    },
                    ListRow {
                        id: "smoke_list_two".into(),
                        title: "Lista 2".into(),
                        description: "Segunda opción".into(),
                    },
                    ListRow {
                        id: "smoke_list_three".into(),
                        title: "Lista 3".into(),
                        description: "Tercera opción".into(),
                    },
                    ListRow {
                        id: "smoke_list_four".into(),
                        title: "Lista 4".into(),
                        description: "Cuarta opción".into(),
                    },
                ],
            }],
        )
        .await
        .expect("send_list should succeed");
}
