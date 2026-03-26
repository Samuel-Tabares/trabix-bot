use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use axum::{
    body::Body,
    extract::{Multipart, Path as AxumPath, State},
    http::{header, HeaderValue, Response, StatusCode},
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::fs;

use crate::{
    bot::{
        state_machine::UserInput,
        timers::{
            simulator_timer_rules, simulator_timer_snapshots, update_simulator_timer_overrides,
            SimulatorTimerOverrides, SimulatorTimerRuleInfo, SimulatorTimerSnapshot,
        },
    },
    db::{
        models::{Conversation, Order, OrderItem},
        queries::{list_conversations, list_order_items, list_orders},
    },
    engine::{process_advisor_input, process_customer_input},
    simulator::{
        create_media, create_message, create_or_update_session, ensure_session_conversation,
        get_media, get_session, list_messages_for_session, list_sessions, snapshot_state,
        NewSimulatorMedia, NewSimulatorMessage, SimulatorSession,
    },
    transport::SIMULATOR_MENU_ASSET_PATH,
    AppState,
};

static NEXT_MEDIA_ID: AtomicU64 = AtomicU64::new(1);
const SIMULATOR_INDEX_HTML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/simulator/index.html"
));
const SIMULATOR_CSS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/simulator/simulator.css"
));
const SIMULATOR_JS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/simulator/simulator.js"
));

pub fn mount(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/simulator", get(simulator_app))
        .route("/simulator/assets/simulator.css", get(simulator_css))
        .route("/simulator/assets/simulator.js", get(simulator_js))
        .route(
            "/simulator/api/sessions",
            get(api_list_sessions).post(api_create_session),
        )
        .route(
            "/simulator/api/timer-overrides",
            get(api_get_timer_overrides).post(api_update_timer_overrides),
        )
        .route("/simulator/api/menu-asset", get(api_menu_asset))
        .route("/simulator/api/media/:id", get(api_media))
        .route("/simulator/api/db/conversations", get(api_db_conversations))
        .route("/simulator/api/db/orders", get(api_db_orders))
        .route("/simulator/api/db/order-items", get(api_db_order_items))
        .route(
            "/simulator/api/sessions/:id/messages",
            get(api_session_messages),
        )
        .route("/simulator/api/sessions/:id/state", get(api_session_state))
        .route(
            "/simulator/api/sessions/:id/customer/text",
            post(api_customer_text),
        )
        .route(
            "/simulator/api/sessions/:id/customer/button",
            post(api_customer_button),
        )
        .route(
            "/simulator/api/sessions/:id/customer/list",
            post(api_customer_list),
        )
        .route(
            "/simulator/api/sessions/:id/customer/image",
            post(api_customer_image),
        )
        .route(
            "/simulator/api/sessions/:id/advisor/text",
            post(api_advisor_text),
        )
        .route(
            "/simulator/api/sessions/:id/advisor/button",
            post(api_advisor_button),
        )
        .route(
            "/simulator/api/sessions/:id/advisor/list",
            post(api_advisor_list),
        )
}

pub async fn simulator_app() -> Html<&'static str> {
    Html(SIMULATOR_INDEX_HTML)
}

pub async fn simulator_css() -> Result<Response<Body>, StatusCode> {
    static_text_response(SIMULATOR_CSS, "text/css; charset=utf-8")
}

pub async fn simulator_js() -> Result<Response<Body>, StatusCode> {
    static_text_response(SIMULATOR_JS, "application/javascript; charset=utf-8")
}

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    customer_phone: String,
    profile_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TextRequest {
    body: String,
}

#[derive(Debug, Deserialize)]
struct SelectionRequest {
    id: String,
}

#[derive(Debug, Serialize)]
struct SessionStateResponse {
    session: SimulatorSession,
    conversation: Option<ConversationSnapshot>,
    generated_at: chrono::DateTime<chrono::Utc>,
    timers: Vec<SimulatorTimerSnapshot>,
}

#[derive(Debug, Serialize)]
struct ConversationSnapshot {
    phone_number: String,
    state: String,
    state_data: crate::db::models::ConversationStateData,
    customer_name: Option<String>,
    customer_phone: Option<String>,
    delivery_address: Option<String>,
    last_message_at: chrono::DateTime<chrono::Utc>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct TimerOverridesResponse {
    rules: Vec<SimulatorTimerRuleInfo>,
}

#[derive(Debug, Serialize)]
struct DbRowsResponse<T> {
    rows: Vec<T>,
    generated_at: chrono::DateTime<chrono::Utc>,
}

async fn api_list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::simulator::SimulatorSessionSummary>>, StatusCode> {
    list_sessions(&state.pool)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_create_session(
    State(state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Json<crate::simulator::SimulatorSessionSummary>, StatusCode> {
    let phone = request.customer_phone.trim();
    if phone.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let session = create_or_update_session(&state.pool, phone, request.profile_name.as_deref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    ensure_session_conversation(&state.pool, &session.customer_phone)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let summary = list_sessions(&state.pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .find(|item| item.id == session.id)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(summary))
}

async fn api_session_messages(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<i32>,
) -> Result<Json<Vec<crate::simulator::SimulatorMessage>>, StatusCode> {
    ensure_session_exists(&state, session_id).await?;
    list_messages_for_session(&state.pool, session_id)
        .await
        .map(Json)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn api_db_conversations(
    State(state): State<AppState>,
) -> Result<Json<DbRowsResponse<Conversation>>, StatusCode> {
    Ok(Json(DbRowsResponse {
        rows: list_conversations(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        generated_at: chrono::Utc::now(),
    }))
}

async fn api_db_orders(
    State(state): State<AppState>,
) -> Result<Json<DbRowsResponse<Order>>, StatusCode> {
    Ok(Json(DbRowsResponse {
        rows: list_orders(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        generated_at: chrono::Utc::now(),
    }))
}

async fn api_db_order_items(
    State(state): State<AppState>,
) -> Result<Json<DbRowsResponse<OrderItem>>, StatusCode> {
    Ok(Json(DbRowsResponse {
        rows: list_order_items(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        generated_at: chrono::Utc::now(),
    }))
}

async fn api_get_timer_overrides(
    State(state): State<AppState>,
) -> Result<Json<TimerOverridesResponse>, StatusCode> {
    Ok(Json(TimerOverridesResponse {
        rules: simulator_timer_rules(&state),
    }))
}

async fn api_update_timer_overrides(
    State(state): State<AppState>,
    Json(request): Json<SimulatorTimerOverrides>,
) -> Result<Json<TimerOverridesResponse>, StatusCode> {
    Ok(Json(TimerOverridesResponse {
        rules: update_simulator_timer_overrides(&state, request),
    }))
}

async fn api_session_state(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<i32>,
) -> Result<Json<SessionStateResponse>, StatusCode> {
    let snapshot = snapshot_state(&state.pool, &state.config.advisor_phone, session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let now = chrono::Utc::now();
    let timers = snapshot
        .conversation
        .as_ref()
        .map(|conversation| simulator_timer_snapshots(&state, conversation, now))
        .unwrap_or_default();

    Ok(Json(SessionStateResponse {
        session: snapshot.session,
        conversation: snapshot
            .conversation
            .map(|conversation| ConversationSnapshot {
                phone_number: conversation.phone_number,
                state: conversation.state,
                state_data: conversation.state_data.0,
                customer_name: conversation.customer_name,
                customer_phone: conversation.customer_phone,
                delivery_address: conversation.delivery_address,
                last_message_at: conversation.last_message_at,
                created_at: conversation.created_at,
            }),
        generated_at: now,
        timers,
    }))
}

async fn api_customer_text(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<i32>,
    Json(request): Json<TextRequest>,
) -> Result<StatusCode, StatusCode> {
    let session = ensure_session_exists(&state, session_id).await?;
    record_inbound_message(
        &state,
        session_id,
        "customer",
        "customer",
        "text",
        Some(request.body.clone()),
        json!({}),
    )
    .await?;
    process_customer_input(
        state,
        session.customer_phone,
        session.profile_name,
        UserInput::TextMessage(request.body),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn api_customer_button(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<i32>,
    Json(request): Json<SelectionRequest>,
) -> Result<StatusCode, StatusCode> {
    let session = ensure_session_exists(&state, session_id).await?;
    record_inbound_message(
        &state,
        session_id,
        "customer",
        "customer",
        "button",
        Some(request.id.clone()),
        json!({ "id": request.id }),
    )
    .await?;
    process_customer_input(
        state,
        session.customer_phone,
        session.profile_name,
        UserInput::ButtonPress(request.id),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn api_customer_list(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<i32>,
    Json(request): Json<SelectionRequest>,
) -> Result<StatusCode, StatusCode> {
    let session = ensure_session_exists(&state, session_id).await?;
    record_inbound_message(
        &state,
        session_id,
        "customer",
        "customer",
        "list",
        Some(request.id.clone()),
        json!({ "id": request.id }),
    )
    .await?;
    process_customer_input(
        state,
        session.customer_phone,
        session.profile_name,
        UserInput::ListSelection(request.id),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn api_customer_image(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<i32>,
    mut multipart: Multipart,
) -> Result<StatusCode, StatusCode> {
    let session = ensure_session_exists(&state, session_id).await?;
    let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let file_name = field.file_name().map(str::to_owned);
    let mime_type = field.content_type().map(str::to_owned);
    let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
    let media_id = next_media_id();
    let relative_name = sanitized_filename(file_name.as_deref().unwrap_or("upload.bin"));
    let stored_path = state
        .config
        .simulator()
        .upload_dir
        .join(format!("{media_id}_{relative_name}"));
    fs::write(&stored_path, bytes)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    create_media(
        &state.pool,
        NewSimulatorMedia {
            id: media_id.clone(),
            session_id: Some(session_id),
            kind: "receipt".to_string(),
            file_path: stored_path.to_string_lossy().to_string(),
            mime_type: mime_type.clone(),
            original_filename: file_name.clone(),
        },
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    record_inbound_message(
        &state,
        session_id,
        "customer",
        "customer",
        "image",
        file_name,
        json!({
            "media_id": media_id,
            "mime_type": mime_type,
            "media_url": format!("/simulator/api/media/{media_id}"),
        }),
    )
    .await?;
    process_customer_input(
        state,
        session.customer_phone,
        session.profile_name,
        UserInput::ImageMessage(media_id),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn api_advisor_text(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<i32>,
    Json(request): Json<TextRequest>,
) -> Result<StatusCode, StatusCode> {
    ensure_session_exists(&state, session_id).await?;
    record_inbound_message(
        &state,
        session_id,
        "advisor",
        "advisor",
        "text",
        Some(request.body.clone()),
        json!({}),
    )
    .await?;
    process_advisor_input(state, UserInput::TextMessage(request.body))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn api_advisor_button(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<i32>,
    Json(request): Json<SelectionRequest>,
) -> Result<StatusCode, StatusCode> {
    ensure_session_exists(&state, session_id).await?;
    record_inbound_message(
        &state,
        session_id,
        "advisor",
        "advisor",
        "button",
        Some(request.id.clone()),
        json!({ "id": request.id }),
    )
    .await?;
    process_advisor_input(state, UserInput::ButtonPress(request.id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn api_advisor_list(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<i32>,
    Json(request): Json<SelectionRequest>,
) -> Result<StatusCode, StatusCode> {
    ensure_session_exists(&state, session_id).await?;
    record_inbound_message(
        &state,
        session_id,
        "advisor",
        "advisor",
        "list",
        Some(request.id.clone()),
        json!({ "id": request.id }),
    )
    .await?;
    process_advisor_input(state, UserInput::ListSelection(request.id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(StatusCode::OK)
}

async fn api_media(
    State(state): State<AppState>,
    AxumPath(media_id): AxumPath<String>,
) -> Result<Response<Body>, StatusCode> {
    let Some(media) = get_media(&state.pool, &media_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    else {
        return Err(StatusCode::NOT_FOUND);
    };
    file_response(PathBuf::from(media.file_path), media.mime_type.as_deref()).await
}

async fn api_menu_asset(State(state): State<AppState>) -> Result<Response<Body>, StatusCode> {
    let _ = state;
    file_response(PathBuf::from(SIMULATOR_MENU_ASSET_PATH), None).await
}

async fn ensure_session_exists(
    state: &AppState,
    session_id: i32,
) -> Result<SimulatorSession, StatusCode> {
    get_session(&state.pool, session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)
}

async fn record_inbound_message(
    state: &AppState,
    session_id: i32,
    actor: &str,
    audience: &str,
    message_kind: &str,
    body: Option<String>,
    payload: serde_json::Value,
) -> Result<(), StatusCode> {
    create_message(
        &state.pool,
        NewSimulatorMessage {
            session_id: Some(session_id),
            actor: actor.to_string(),
            audience: audience.to_string(),
            message_kind: message_kind.to_string(),
            body,
            payload,
        },
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(())
}

fn static_text_response(
    body: &'static str,
    content_type: &'static str,
) -> Result<Response<Body>, StatusCode> {
    let mut response = Response::new(Body::from(body));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    Ok(response)
}

async fn file_response(
    path: PathBuf,
    mime_type: Option<&str>,
) -> Result<Response<Body>, StatusCode> {
    let bytes = fs::read(&path).await.map_err(|_| StatusCode::NOT_FOUND)?;
    let guessed = mime_type
        .map(str::to_owned)
        .unwrap_or_else(|| content_type_for_path(&path).to_string());
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&guessed).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    );
    Ok(response)
}

fn next_media_id() -> String {
    format!(
        "sim_media_{}",
        NEXT_MEDIA_ID.fetch_add(1, Ordering::Relaxed)
    )
}

fn sanitized_filename(name: &str) -> String {
    name.chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => ch,
            _ => '_',
        })
        .collect()
}

fn content_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "image/jpeg",
    }
}
