use std::time::{SystemTime, UNIX_EPOCH};

use granizado_bot::db::{
    models::ConversationStateData,
    queries::{
        create_conversation, get_conversation, update_customer_data, update_state,
    },
};

fn unique_phone_number() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis();
    let suffix = millis % 1_000_000_000;
    format!("57{suffix:09}")
}

fn load_env() {
    let _ = dotenvy::dotenv();
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn migrates_and_exercises_basic_conversation_crud() {
    load_env();
    let database_url =
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set for live DB smoke tests");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&database_url)
        .await
        .expect("db connection");

    sqlx::migrate!().run(&pool).await.expect("migrations");

    let phone_number = unique_phone_number();
    let created = create_conversation(&pool, &phone_number)
        .await
        .expect("create conversation");

    assert_eq!(created.phone_number, phone_number);
    assert_eq!(created.state, "main_menu");

    let loaded = get_conversation(&pool, &phone_number)
        .await
        .expect("load conversation")
        .expect("conversation should exist");
    assert_eq!(loaded.phone_number, phone_number);

    let state_data = ConversationStateData {
        delivery_type: Some("immediate".into()),
        ..ConversationStateData::default()
    };
    update_state(&pool, &phone_number, "collect_name", &state_data)
        .await
        .expect("update state");

    let updated = get_conversation(&pool, &phone_number)
        .await
        .expect("reload conversation")
        .expect("conversation should still exist");
    assert_eq!(updated.state, "collect_name");
    assert_eq!(updated.state_data.0.delivery_type.as_deref(), Some("immediate"));
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn persists_phase2_progress_and_customer_columns() {
    load_env();
    let database_url =
        std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set for live DB smoke tests");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&database_url)
        .await
        .expect("db connection");

    sqlx::migrate!().run(&pool).await.expect("migrations");

    let phone_number = unique_phone_number();
    create_conversation(&pool, &phone_number)
        .await
        .expect("create conversation");

    update_customer_data(
        &pool,
        &phone_number,
        Some("Ana Maria"),
        Some("3001234567"),
        Some("Cra 15 #20-30 Armenia"),
    )
    .await
    .expect("update customer data");

    let state_data = ConversationStateData {
        items: vec![
            granizado_bot::db::models::OrderItemData {
                flavor: "maracuya".into(),
                has_liquor: true,
                quantity: 2,
            },
            granizado_bot::db::models::OrderItemData {
                flavor: "mora".into(),
                has_liquor: false,
                quantity: 1,
            },
            granizado_bot::db::models::OrderItemData {
                flavor: "fresa".into(),
                has_liquor: true,
                quantity: 4,
            },
        ],
        delivery_type: Some("scheduled".into()),
        scheduled_date: Some("2030-12-24".into()),
        scheduled_time: Some("15:45".into()),
        pending_has_liquor: Some(true),
        pending_flavor: Some("fresa".into()),
        ..ConversationStateData::default()
    };

    update_state(&pool, &phone_number, "show_summary", &state_data)
        .await
        .expect("update phase2 state");

    let updated = get_conversation(&pool, &phone_number)
        .await
        .expect("reload conversation")
        .expect("conversation should still exist");

    assert_eq!(updated.state, "show_summary");
    assert_eq!(updated.customer_name.as_deref(), Some("Ana Maria"));
    assert_eq!(updated.customer_phone.as_deref(), Some("3001234567"));
    assert_eq!(
        updated.delivery_address.as_deref(),
        Some("Cra 15 #20-30 Armenia")
    );
    assert_eq!(updated.state_data.0.items.len(), 3);
    assert_eq!(updated.state_data.0.scheduled_date.as_deref(), Some("2030-12-24"));
    assert_eq!(updated.state_data.0.scheduled_time.as_deref(), Some("15:45"));
    assert_eq!(updated.state_data.0.pending_has_liquor, Some(true));
    assert_eq!(updated.state_data.0.pending_flavor.as_deref(), Some("fresa"));
}
