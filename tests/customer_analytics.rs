use sqlx::PgPool;
use std::time::{SystemTime, UNIX_EPOCH};

async fn setup_db() -> PgPool {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for ignored DB tests");

    let pool = PgPool::connect(&database_url)
        .await
        .expect("db connection");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations");

    pool
}

fn unique_phone() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .subsec_nanos() as u64;
    format!("573{:010}", nanos % 10_000_000_000)
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn test_customer_totals_update_on_order_confirmation() {
    use granizado_bot::db::queries::{
        create_or_update_customer, get_customer, update_customer_totals,
    };

    let pool = setup_db().await;
    let phone = unique_phone();

    // Create initial customer
    create_or_update_customer(
        &pool,
        &phone,
        Some(&phone),
        Some("Juan Pérez"),
        None,
        None,
        None,
    )
    .await
    .expect("create customer");

    // Verify initial state
    let customer = get_customer(&pool, &phone)
        .await
        .expect("get customer")
        .expect("customer exists");
    assert_eq!(customer.total_spent_cop, 0);
    assert_eq!(customer.total_units_purchased, 0);

    // Simulate order confirmation: customer orders 5 units for 45,000 COP
    update_customer_totals(&pool, &phone, 45000, 5)
        .await
        .expect("update totals");

    // Verify totals were updated
    let customer = get_customer(&pool, &phone)
        .await
        .expect("get customer")
        .expect("customer exists");
    assert_eq!(customer.total_spent_cop, 45000);
    assert_eq!(customer.total_units_purchased, 5);

    // Simulate another order: 3 units for 27,000 COP
    update_customer_totals(&pool, &phone, 27000, 3)
        .await
        .expect("update totals again");

    // Verify totals were incremented
    let customer = get_customer(&pool, &phone)
        .await
        .expect("get customer")
        .expect("customer exists");
    assert_eq!(customer.total_spent_cop, 72000); // 45000 + 27000
    assert_eq!(customer.total_units_purchased, 8); // 5 + 3
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a reachable PostgreSQL instance"]
async fn test_referral_analytics_update_on_order_confirmation() {
    use granizado_bot::db::queries::{
        create_or_update_referral_analytics, get_referral_code_analytics,
    };

    let pool = setup_db().await;
    let phone = unique_phone();
    let code = format!("t{}", &phone[phone.len() - 10..]);

    // First order with referral code: 5 units, 5000 COP discount, 10000 COP commission
    create_or_update_referral_analytics(
        &pool,
        &code,
        1,      // times_used increment
        5000,   // discount increment
        10000,  // commission increment
        5,      // units increment
        40000,  // sales increment
    )
    .await
    .expect("create referral analytics");

    // Verify initial state
    let analytics = get_referral_code_analytics(&pool, &code)
        .await
        .expect("get analytics")
        .expect("analytics exists");
    assert_eq!(analytics.times_used, 1);
    assert_eq!(analytics.total_discount_generated_cop, 5000);
    assert_eq!(analytics.total_commission_generated_cop, 10000);
    assert_eq!(analytics.total_units_purchased, 5);
    assert_eq!(analytics.total_sales_cop, 40000);

    // Second order with same referral code: 3 units, 3000 COP discount, 6000 COP commission
    create_or_update_referral_analytics(
        &pool,
        &code,
        1,      // times_used increment
        3000,   // discount increment
        6000,   // commission increment
        3,      // units increment
        24000,  // sales increment
    )
    .await
    .expect("create referral analytics again");

    // Verify totals were incremented
    let analytics = get_referral_code_analytics(&pool, &code)
        .await
        .expect("get analytics")
        .expect("analytics exists");
    assert_eq!(analytics.times_used, 2);
    assert_eq!(analytics.total_discount_generated_cop, 8000); // 5000 + 3000
    assert_eq!(analytics.total_commission_generated_cop, 16000); // 10000 + 6000
    assert_eq!(analytics.total_units_purchased, 8); // 5 + 3
    assert_eq!(analytics.total_sales_cop, 64000); // 40000 + 24000
}
