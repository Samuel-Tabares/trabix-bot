use std::time::{SystemTime, UNIX_EPOCH};

use granizado_bot::db::{
    models::ConversationStateData,
    queries::{create_conversation, get_conversation, update_state},
};

fn unique_phone_number() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis();
    let suffix = millis % 1_000_000_000;
    format!("57{suffix:09}")
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn migrates_and_exercises_basic_conversation_crud() {
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
