use std::time::{SystemTime, UNIX_EPOCH};

use granizado_bot::{
    bot::{
        pricing::calcular_pedido,
        state_machine::TimerType,
        timers::{cancel_timer, new_timer_map, restore_pending_timers},
    },
    config::Config,
    db::{
        models::{ConversationStateData, OrderItemData},
        queries::{
            create_conversation, create_order, get_conversation, get_order, get_order_items,
            replace_order_items, update_customer_data, update_order_receipt_media_id,
            update_order_status, update_state,
        },
    },
    whatsapp::client::WhatsAppClient,
    AppState,
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
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for live DB smoke tests");
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
    assert_eq!(
        updated.state_data.0.delivery_type.as_deref(),
        Some("immediate")
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn persists_phase2_progress_and_customer_columns() {
    load_env();
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for live DB smoke tests");
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
    assert_eq!(
        updated.state_data.0.scheduled_date.as_deref(),
        Some("2030-12-24")
    );
    assert_eq!(
        updated.state_data.0.scheduled_time.as_deref(),
        Some("15:45")
    );
    assert_eq!(updated.state_data.0.pending_has_liquor, Some(true));
    assert_eq!(
        updated.state_data.0.pending_flavor.as_deref(),
        Some("fresa")
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn persists_phase3_order_items_and_receipt_media() {
    load_env();
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for live DB smoke tests");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&database_url)
        .await
        .expect("db connection");

    sqlx::migrate!().run(&pool).await.expect("migrations");

    let phone_number = unique_phone_number();
    let conversation = create_conversation(&pool, &phone_number)
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

    let pedido = calcular_pedido(&[
        OrderItemData {
            flavor: "maracuya".into(),
            has_liquor: true,
            quantity: 2,
        },
        OrderItemData {
            flavor: "mora".into(),
            has_liquor: false,
            quantity: 1,
        },
    ]);

    let order = create_order(
        &pool,
        conversation.id,
        "immediate",
        None,
        None,
        "transfer",
        None,
        i32::try_from(pedido.total_estimado).expect("total fits i32"),
    )
    .await
    .expect("create order");

    update_order_status(&pool, order.id, "waiting_receipt")
        .await
        .expect("set waiting receipt");

    let persisted_items = pedido
        .items_detalle
        .iter()
        .flat_map(|item| item.persistence_lines.iter())
        .map(|line| {
            (
                line.flavor.clone(),
                line.has_liquor,
                i32::try_from(line.quantity).expect("quantity fits i32"),
                i32::try_from(line.unit_price).expect("unit price fits i32"),
                i32::try_from(line.subtotal).expect("subtotal fits i32"),
            )
        })
        .collect::<Vec<_>>();

    replace_order_items(&pool, order.id, &persisted_items)
        .await
        .expect("replace order items");
    update_order_receipt_media_id(&pool, order.id, Some("media-123"))
        .await
        .expect("save receipt media");
    update_order_status(&pool, order.id, "pending_advisor")
        .await
        .expect("set pending advisor");

    let stored_order = get_order(&pool, order.id)
        .await
        .expect("load order")
        .expect("order exists");
    let stored_items = get_order_items(&pool, order.id)
        .await
        .expect("load order items");

    assert_eq!(stored_order.status, "pending_advisor");
    assert_eq!(stored_order.receipt_media_id.as_deref(), Some("media-123"));
    assert_eq!(stored_order.total_estimated, 19_000);
    assert_eq!(stored_items.len(), 3);
    assert_eq!(
        stored_items.iter().map(|item| item.subtotal).sum::<i32>(),
        19_000
    );
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn recreates_wait_receipt_timer_after_restart() {
    load_env();
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for live DB smoke tests");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&database_url)
        .await
        .expect("db connection");

    sqlx::migrate!().run(&pool).await.expect("migrations");

    let phone_number = unique_phone_number();
    create_conversation(&pool, &phone_number)
        .await
        .expect("create conversation");

    let state_data = ConversationStateData {
        payment_method: Some("transfer".into()),
        current_order_id: Some(99),
        ..ConversationStateData::default()
    };

    update_state(&pool, &phone_number, "wait_receipt", &state_data)
        .await
        .expect("set wait receipt");

    let app_state = AppState {
        config: Config {
            whatsapp_token: "token".into(),
            whatsapp_phone_id: "phone".into(),
            whatsapp_verify_token: "verify".into(),
            whatsapp_app_secret: "secret".into(),
            database_url,
            advisor_phone: "573001234567".into(),
            transfer_payment_text: "Nequi 3001234567".into(),
            menu_image_media_id: "menu-media".into(),
            port: 8080,
        },
        pool,
        wa_client: WhatsAppClient::new("token".into(), "phone".into()),
        timers: new_timer_map(),
    };

    restore_pending_timers(app_state.clone())
        .await
        .expect("restore timers");

    let active = app_state.timers.lock().await;
    assert!(active.contains_key(&(phone_number.clone(), TimerType::ReceiptUpload)));
    drop(active);

    cancel_timer(
        app_state.timers.clone(),
        &(phone_number, TimerType::ReceiptUpload),
    )
    .await;
}
