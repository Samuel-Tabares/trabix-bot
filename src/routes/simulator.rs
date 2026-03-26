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

pub fn mount(router: Router<AppState>) -> Router<AppState> {
    router
        .route("/simulator", get(simulator_app))
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

pub async fn simulator_app() -> Html<String> {
    Html(render_simulator_html())
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

fn render_simulator_html() -> String {
    r#"<!DOCTYPE html>
<html lang="es">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Trabix Simulator</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f4ede2;
      --panel: rgba(255, 250, 243, 0.92);
      --line: #d9c4a6;
      --ink: #2a1f17;
      --muted: #715f4c;
      --accent: #bf5a24;
      --accent-2: #1f6c5c;
      --shadow: 0 20px 55px rgba(71, 45, 22, 0.15);
      --radius: 22px;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      min-height: 100vh;
      background:
        radial-gradient(circle at top left, rgba(191, 90, 36, 0.18), transparent 28%),
        radial-gradient(circle at top right, rgba(31, 108, 92, 0.14), transparent 30%),
        linear-gradient(180deg, #f9f3e8 0%, #efe4d4 100%);
      color: var(--ink);
      font-family: Georgia, "Iowan Old Style", "Times New Roman", serif;
    }
    .shell {
      display: grid;
      grid-template-columns: 320px 1fr;
      gap: 18px;
      padding: 18px;
    }
    .panel {
      background: var(--panel);
      backdrop-filter: blur(10px);
      border: 1px solid rgba(141, 105, 73, 0.2);
      border-radius: var(--radius);
      box-shadow: var(--shadow);
    }
    .sidebar {
      padding: 18px;
      display: flex;
      flex-direction: column;
      gap: 14px;
      min-height: calc(100vh - 36px);
    }
    .brand {
      padding: 8px 4px 2px;
    }
    .eyebrow {
      margin: 0;
      color: var(--accent);
      font-size: 0.8rem;
      text-transform: uppercase;
      letter-spacing: 0.14em;
      font-weight: 700;
    }
    h1 {
      margin: 6px 0 0;
      font-size: 2rem;
      line-height: 1;
    }
    .muted { color: var(--muted); }
    form, .box {
      border: 1px solid var(--line);
      border-radius: 18px;
      padding: 14px;
      background: rgba(255,255,255,0.55);
    }
    input, textarea, button, select {
      font: inherit;
    }
    input, textarea {
      width: 100%;
      border: 1px solid #d3b995;
      border-radius: 12px;
      padding: 11px 12px;
      background: #fffdf9;
      color: var(--ink);
    }
    textarea { min-height: 90px; resize: vertical; }
    button {
      border: 0;
      border-radius: 999px;
      padding: 11px 14px;
      background: var(--accent);
      color: white;
      cursor: pointer;
      transition: transform 120ms ease, opacity 120ms ease;
    }
    button:hover { transform: translateY(-1px); }
    button.secondary { background: var(--accent-2); }
    button.ghost {
      background: #fff8ef;
      color: var(--ink);
      border: 1px solid var(--line);
    }
    .session-list {
      display: flex;
      flex-direction: column;
      gap: 10px;
      max-height: 44vh;
      overflow: auto;
      padding-right: 2px;
    }
    .session-card {
      border: 1px solid var(--line);
      border-radius: 16px;
      padding: 12px;
      background: #fffdf9;
      cursor: pointer;
    }
    .session-card.active {
      border-color: var(--accent);
      box-shadow: inset 0 0 0 1px rgba(191, 90, 36, 0.25);
      background: #fff5ec;
    }
    .layout {
      display: grid;
      grid-template-rows: auto 1fr auto;
      gap: 18px;
      min-height: calc(100vh - 36px);
    }
    .header {
      padding: 18px;
      display: grid;
      grid-template-columns: 1.15fr 0.95fr 1.1fr;
      gap: 18px;
      align-items: start;
    }
    .header-grid {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 10px;
      font-family: "SFMono-Regular", "Menlo", monospace;
      font-size: 0.88rem;
    }
    .header-column {
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    .stack {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 18px;
      min-height: 0;
      padding: 0 18px 18px;
    }
    .db-panel {
      margin: 0 18px 18px;
      padding: 18px;
      display: flex;
      flex-direction: column;
      gap: 14px;
      min-height: 280px;
    }
    .db-head {
      display: flex;
      justify-content: space-between;
      gap: 12px;
      align-items: center;
      flex-wrap: wrap;
    }
    .db-tabs {
      display: flex;
      gap: 10px;
      flex-wrap: wrap;
    }
    .db-tab {
      background: #fff8ef;
      color: var(--ink);
      border: 1px solid var(--line);
    }
    .db-tab.active {
      background: var(--accent-2);
      color: white;
      border-color: rgba(31, 108, 92, 0.4);
    }
    .db-meta {
      font-family: "SFMono-Regular", "Menlo", monospace;
      font-size: 0.76rem;
      color: var(--muted);
    }
    .db-table-wrap {
      overflow: auto;
      border: 1px solid rgba(217, 196, 166, 0.8);
      border-radius: 18px;
      background: rgba(255,255,255,0.62);
    }
    .db-table {
      width: 100%;
      min-width: 980px;
      border-collapse: collapse;
      font-family: "SFMono-Regular", "Menlo", monospace;
      font-size: 0.8rem;
    }
    .db-table th,
    .db-table td {
      padding: 10px 12px;
      border-bottom: 1px solid rgba(217, 196, 166, 0.65);
      text-align: left;
      vertical-align: top;
      white-space: pre-wrap;
      word-break: break-word;
    }
    .db-table th {
      position: sticky;
      top: 0;
      background: #f8efe1;
      color: var(--accent);
      text-transform: uppercase;
      letter-spacing: 0.08em;
      font-size: 0.73rem;
      z-index: 1;
    }
    .db-empty {
      padding: 18px;
      color: var(--muted);
      font-family: "SFMono-Regular", "Menlo", monospace;
    }
    .mono {
      font-family: "SFMono-Regular", "Menlo", monospace;
    }
    .chat {
      min-height: 0;
      display: grid;
      grid-template-rows: auto 1fr auto;
      overflow: hidden;
    }
    .chat-head {
      padding: 16px 18px 10px;
      border-bottom: 1px solid rgba(217, 196, 166, 0.8);
    }
    .transcript {
      overflow: auto;
      padding: 18px;
      display: flex;
      flex-direction: column;
      gap: 12px;
      background:
        linear-gradient(180deg, rgba(255,255,255,0.32), transparent 30%),
        rgba(255,255,255,0.18);
    }
    .composer {
      border-top: 1px solid rgba(217, 196, 166, 0.8);
      padding: 14px 18px 18px;
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    .msg {
      border-radius: 18px;
      padding: 12px 14px;
      max-width: 88%;
      border: 1px solid rgba(217, 196, 166, 0.75);
      background: #fffdf9;
      box-shadow: 0 10px 24px rgba(64, 40, 22, 0.08);
    }
    .msg.customer { align-self: flex-end; background: #fff1e5; }
    .msg.bot { align-self: flex-start; }
    .msg.advisor { align-self: flex-end; background: #eaf7f2; }
    .msg.system { align-self: center; background: #f3eee7; }
    .meta {
      margin-bottom: 6px;
      display: flex;
      justify-content: space-between;
      gap: 8px;
      align-items: center;
      flex-wrap: wrap;
      font-family: "SFMono-Regular", "Menlo", monospace;
      font-size: 0.75rem;
      color: var(--muted);
      text-transform: uppercase;
      letter-spacing: 0.08em;
    }
    .stamp {
      font-family: "SFMono-Regular", "Menlo", monospace;
      font-size: 0.74rem;
      color: var(--muted);
    }
    .actions {
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
      margin-top: 10px;
    }
    .actions button {
      padding: 8px 10px;
      font-size: 0.92rem;
    }
    .img-preview {
      max-width: 100%;
      border-radius: 14px;
      border: 1px solid rgba(217, 196, 166, 0.75);
      margin-top: 10px;
    }
    .timer-list {
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    .timer-card {
      border: 1px solid var(--line);
      border-radius: 16px;
      padding: 12px;
      background:
        linear-gradient(135deg, rgba(191, 90, 36, 0.08), rgba(255,255,255,0.75)),
        #fffdf9;
    }
    .timer-card.expired {
      border-color: #b8482f;
      background:
        linear-gradient(135deg, rgba(184, 72, 47, 0.12), rgba(255,255,255,0.8)),
        #fff6f2;
    }
    .timer-top {
      display: flex;
      justify-content: space-between;
      gap: 10px;
      align-items: baseline;
      margin-bottom: 8px;
    }
    .countdown {
      font-family: "SFMono-Regular", "Menlo", monospace;
      font-size: 1rem;
      font-weight: 700;
      color: var(--accent-2);
    }
    .expired .countdown {
      color: #b8482f;
    }
    .timer-grid, .override-grid {
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 10px;
    }
    .timer-note {
      font-family: "SFMono-Regular", "Menlo", monospace;
      font-size: 0.78rem;
      color: var(--muted);
      white-space: pre-wrap;
      word-break: break-word;
    }
    .override-card {
      border: 1px solid var(--line);
      border-radius: 16px;
      padding: 12px;
      background: rgba(255,255,255,0.72);
    }
    .override-card label {
      display: block;
      font-size: 0.84rem;
      margin-bottom: 8px;
    }
    .override-card input {
      font-family: "SFMono-Regular", "Menlo", monospace;
      margin-bottom: 8px;
    }
    .override-meta {
      font-family: "SFMono-Regular", "Menlo", monospace;
      font-size: 0.75rem;
      color: var(--muted);
      line-height: 1.45;
    }
    @media (max-width: 1100px) {
      .shell, .stack, .header { grid-template-columns: 1fr; }
      .sidebar, .layout { min-height: auto; }
      .session-list { max-height: none; }
      .timer-grid, .override-grid, .header-grid { grid-template-columns: 1fr; }
    }
  </style>
</head>
<body>
  <div class="shell">
    <aside class="panel sidebar">
      <div class="brand">
        <p class="eyebrow">Local Transport</p>
        <h1>Trabix Simulator</h1>
        <p class="muted">Misma lógica del bot, sin Meta.</p>
      </div>
      <form id="create-session-form">
        <label>Teléfono</label>
        <input id="new-phone" name="customer_phone" placeholder="573001112233" required>
        <label style="margin-top:10px; display:block;">Nombre de perfil</label>
        <input id="new-name" name="profile_name" placeholder="Cliente Local">
        <button style="margin-top:12px; width:100%;">Crear sesión</button>
      </form>
      <div class="box">
        <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:10px;">
          <strong>Sesiones</strong>
          <button class="ghost" id="refresh-sessions" type="button">Refrescar</button>
        </div>
        <div id="session-list" class="session-list"></div>
      </div>
      <div class="box">
        <div style="display:flex; justify-content:space-between; align-items:center; gap:10px; margin-bottom:10px;">
          <strong>Ritmo local</strong>
          <button class="ghost" id="reset-overrides" type="button">Restaurar</button>
        </div>
        <p class="muted" style="margin-top:0;">Ajusta solo el simulador. Los cambios aplican a timers nuevos.</p>
        <div id="override-grid" class="override-grid"></div>
      </div>
    </aside>
    <main class="layout">
      <section class="panel header">
        <div class="header-column">
          <p class="eyebrow">Estado Persistido</p>
          <div id="state-grid" class="header-grid"></div>
        </div>
        <div class="header-column">
          <p class="eyebrow">Sesión Seleccionada</p>
          <div id="session-summary" class="header-grid"></div>
        </div>
        <div class="header-column">
          <p class="eyebrow">Timers Activos</p>
          <div id="timer-list" class="timer-list"></div>
        </div>
      </section>
      <section class="stack">
        <article class="panel chat">
          <div class="chat-head">
            <p class="eyebrow">Cliente</p>
            <div class="muted">Mensajes del cliente y respuestas del bot.</div>
          </div>
          <div id="customer-transcript" class="transcript"></div>
          <div class="composer">
            <textarea id="customer-text" placeholder="Escribe como cliente..."></textarea>
            <div style="display:flex; gap:10px; flex-wrap:wrap;">
              <button id="send-customer-text" type="button">Enviar texto</button>
              <input id="customer-image" type="file" accept="image/*">
              <button id="send-customer-image" class="ghost" type="button">Enviar imagen</button>
            </div>
          </div>
        </article>
        <article class="panel chat">
          <div class="chat-head">
            <p class="eyebrow">Asesor</p>
            <div class="muted">Mensajes del asesor, respuestas del bot y timeouts de esta sesión.</div>
          </div>
          <div id="advisor-transcript" class="transcript"></div>
          <div class="composer">
            <textarea id="advisor-text" placeholder="Escribe como asesor..."></textarea>
            <div style="display:flex; gap:10px; flex-wrap:wrap;">
              <button id="send-advisor-text" class="secondary" type="button">Enviar texto</button>
            </div>
          </div>
        </article>
      </section>
      <section class="panel db-panel">
        <div class="db-head">
          <div>
            <p class="eyebrow">Base de Datos Local</p>
            <div class="muted">Vista cruda de `conversations`, `orders` y `order_items`.</div>
          </div>
          <div style="display:flex; gap:10px; align-items:center; flex-wrap:wrap;">
            <div id="db-meta" class="db-meta"></div>
            <button class="ghost" id="refresh-db" type="button">Refrescar DB</button>
          </div>
        </div>
        <div id="db-tabs" class="db-tabs"></div>
        <div id="db-table-wrap" class="db-table-wrap"></div>
      </section>
    </main>
  </div>
  <script>
    const state = {
      sessions: [],
      selectedSessionId: null,
      selectedMessages: [],
      selectedSnapshot: null,
      snapshotFetchedAt: null,
      timerRules: [],
      activeDbTab: 'conversations',
      dbRows: {
        conversations: [],
        orders: [],
        order_items: [],
      },
      dbGeneratedAt: null,
      refreshLocks: {},
    };
    const TIMER_FIELDS = [
      { key: 'advisor_response', field: 'advisor_response_seconds' },
      { key: 'receipt_upload', field: 'receipt_upload_seconds' },
      { key: 'advisor_stuck', field: 'advisor_stuck_seconds' },
      { key: 'relay_inactivity', field: 'relay_inactivity_seconds' },
      { key: 'conversation_reminder', field: 'conversation_reminder_seconds' },
      { key: 'conversation_reset', field: 'conversation_reset_seconds' },
    ];
    const DB_TABLES = [
      {
        key: 'conversations',
        label: 'conversations',
        endpoint: '/simulator/api/db/conversations',
        columns: ['id', 'phone_number', 'state', 'state_data', 'customer_name', 'customer_phone', 'delivery_address', 'last_message_at', 'created_at'],
      },
      {
        key: 'orders',
        label: 'orders',
        endpoint: '/simulator/api/db/orders',
        columns: ['id', 'conversation_id', 'delivery_type', 'scheduled_date', 'scheduled_time', 'scheduled_date_text', 'scheduled_time_text', 'payment_method', 'receipt_media_id', 'delivery_cost', 'total_estimated', 'total_final', 'status', 'created_at'],
      },
      {
        key: 'order_items',
        label: 'order_items',
        endpoint: '/simulator/api/db/order-items',
        columns: ['id', 'order_id', 'flavor', 'has_liquor', 'quantity', 'unit_price', 'subtotal', 'created_at'],
      },
    ];
    const bogotaFormatter = new Intl.DateTimeFormat('es-CO', {
      timeZone: 'America/Bogota',
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    });

    const sessionList = document.getElementById('session-list');
    const customerTranscript = document.getElementById('customer-transcript');
    const advisorTranscript = document.getElementById('advisor-transcript');
    const stateGrid = document.getElementById('state-grid');
    const sessionSummary = document.getElementById('session-summary');
    const timerList = document.getElementById('timer-list');
    const overrideGrid = document.getElementById('override-grid');
    const dbTabs = document.getElementById('db-tabs');
    const dbTableWrap = document.getElementById('db-table-wrap');
    const dbMeta = document.getElementById('db-meta');

    async function fetchJson(url, options) {
      const response = await fetch(url, options);
      if (!response.ok) {
        throw new Error(`request failed: ${url}`);
      }
      if (response.status === 204) return null;
      return response.json();
    }

    async function withRefreshLock(key, fn) {
      if (state.refreshLocks[key]) return;
      state.refreshLocks[key] = true;
      try {
        await fn();
      } finally {
        state.refreshLocks[key] = false;
      }
    }

    async function refreshSessionsList() {
      state.sessions = await fetchJson('/simulator/api/sessions');
      if (!state.selectedSessionId && state.sessions.length) {
        state.selectedSessionId = state.sessions[0].id;
      }
      if (state.selectedSessionId && !state.sessions.some((session) => session.id === state.selectedSessionId)) {
        state.selectedSessionId = state.sessions[0]?.id ?? null;
      }
      renderSessions();
      if (!state.selectedSessionId) {
        renderEmptySessionState();
      }
    }

    async function refreshSelectedSession(forceScroll = false) {
      if (!state.selectedSessionId) {
        renderEmptySessionState();
        return;
      }
      const [messages, snapshot] = await Promise.all([
        fetchJson(`/simulator/api/sessions/${state.selectedSessionId}/messages`),
        fetchJson(`/simulator/api/sessions/${state.selectedSessionId}/state`),
      ]);
      state.selectedMessages = messages;
      state.selectedSnapshot = snapshot;
      state.snapshotFetchedAt = Date.now();
      renderState(snapshot);
      renderTranscripts(forceScroll);
    }

    async function loadTimerRules() {
      const response = await fetchJson('/simulator/api/timer-overrides');
      state.timerRules = response.rules || [];
      renderTimerOverrides();
    }

    async function refreshDbTable(tabKey = state.activeDbTab) {
      const definition = DB_TABLES.find((table) => table.key === tabKey) || DB_TABLES[0];
      state.activeDbTab = definition.key;
      const response = await fetchJson(definition.endpoint);
      state.dbRows[definition.key] = response.rows || [];
      state.dbGeneratedAt = response.generated_at || null;
      renderDbPanel();
    }

    async function hardRefresh(forceScroll = false) {
      await refreshSessionsList();
      await refreshSelectedSession(forceScroll);
      await refreshDbTable();
    }

    function renderEmptySessionState() {
      state.selectedMessages = [];
      state.selectedSnapshot = null;
      state.snapshotFetchedAt = null;
      customerTranscript.innerHTML = '<div class="box muted">Crea o selecciona una sesión.</div>';
      advisorTranscript.innerHTML = '<div class="box muted">Selecciona una sesión para usar el panel del asesor.</div>';
      timerList.innerHTML = '<div class="box muted">Selecciona una sesión para ver timers.</div>';
      stateGrid.innerHTML = '';
      sessionSummary.innerHTML = '';
    }

    function renderSessions() {
      sessionList.innerHTML = state.sessions.map((session) => `
        <div class="session-card ${session.id === state.selectedSessionId ? 'active' : ''}" data-session-id="${session.id}">
          <div><strong>${escapeHtml(session.profile_name || 'Sin nombre')}</strong></div>
          <div class="muted mono">${escapeHtml(session.customer_phone)}</div>
          <div class="muted">Estado: ${escapeHtml(session.state)}</div>
          <div class="muted">Dirección: ${escapeHtml(session.delivery_address || 'Pendiente')}</div>
          <div class="stamp">${escapeHtml(formatBogotaTimestamp(session.updated_at))}</div>
        </div>
      `).join('');
      for (const element of sessionList.querySelectorAll('[data-session-id]')) {
        element.addEventListener('click', async () => {
          const sessionId = Number(element.dataset.sessionId);
          if (Number.isNaN(sessionId) || sessionId === state.selectedSessionId) return;
          state.selectedSessionId = sessionId;
          renderSessions();
          await refreshSelectedSession(true);
        });
      }
    }

    function renderState(snapshot) {
      const conversation = snapshot.conversation || {};
      const entries = {
        state: conversation.state || 'main_menu',
        customer_name: conversation.customer_name || '(vacío)',
        customer_phone: conversation.customer_phone || '(vacío)',
        delivery_address: conversation.delivery_address || '(vacío)',
        current_order_id: conversation.state_data?.current_order_id ?? '(vacío)',
        advisor_target_phone: conversation.state_data?.advisor_target_phone || '(vacío)',
        receipt_timer_expired: String(conversation.state_data?.receipt_timer_expired ?? false),
        advisor_timer_expired: String(conversation.state_data?.advisor_timer_expired ?? false),
      };
      const sessionEntries = {
        session_id: snapshot.session.id,
        profile_name: snapshot.session.profile_name || '(vacío)',
        session_phone: snapshot.session.customer_phone,
        created_at: formatBogotaTimestamp(snapshot.session.created_at),
        updated_at: formatBogotaTimestamp(snapshot.session.updated_at),
        generated_at: formatBogotaTimestamp(snapshot.generated_at),
      };
      stateGrid.innerHTML = Object.entries(entries).map(([key, value]) => `
        <div class="box"><strong>${escapeHtml(key)}</strong><div class="timer-note">${escapeHtml(String(value))}</div></div>
      `).join('');
      sessionSummary.innerHTML = Object.entries(sessionEntries).map(([key, value]) => `
        <div class="box"><strong>${escapeHtml(key)}</strong><div class="timer-note">${escapeHtml(String(value))}</div></div>
      `).join('');
      renderTimerList(snapshot);
    }

    function renderTimerList(snapshot = state.selectedSnapshot) {
      if (!snapshot || !Array.isArray(snapshot.timers) || !snapshot.timers.length) {
        timerList.innerHTML = '<div class="box muted">No hay timers activos para esta sesión.</div>';
        return;
      }
      const elapsedClientSeconds = state.snapshotFetchedAt
        ? Math.max(0, Math.floor((Date.now() - state.snapshotFetchedAt) / 1000))
        : 0;
      timerList.innerHTML = snapshot.timers.map((timer) => {
        const remaining = Math.max(0, (timer.remaining_seconds || 0) - elapsedClientSeconds);
        const expired = timer.expired || remaining <= 0;
        return `
          <div class="timer-card ${expired ? 'expired' : ''}">
            <div class="timer-top">
              <strong>${escapeHtml(timer.label)}</strong>
              <span class="countdown">${expired ? 'Vencido' : escapeHtml(formatCountdown(remaining))}</span>
            </div>
            <div class="timer-grid">
              <div class="box"><strong>rule</strong><div class="timer-note">${escapeHtml(timer.rule_key)}</div></div>
              <div class="box"><strong>phase</strong><div class="timer-note">${escapeHtml(timer.phase)}</div></div>
              <div class="box"><strong>state</strong><div class="timer-note">${escapeHtml(timer.state)}</div></div>
              <div class="box"><strong>window</strong><div class="timer-note">${escapeHtml(String(timer.effective_seconds))} s</div></div>
              <div class="box"><strong>started</strong><div class="timer-note">${escapeHtml(formatBogotaTimestamp(timer.started_at))}</div></div>
              <div class="box"><strong>expires</strong><div class="timer-note">${escapeHtml(formatBogotaTimestamp(timer.expires_at))}</div></div>
            </div>
          </div>
        `;
      }).join('');
    }

    function renderTimerOverrides() {
      overrideGrid.innerHTML = state.timerRules.map((rule) => `
        <div class="override-card">
          <label for="override-${rule.key}">${escapeHtml(rule.label)}</label>
          <input
            id="override-${rule.key}"
            data-override-field="${lookupTimerField(rule.key)}"
            type="number"
            min="1"
            step="1"
            placeholder="${rule.default_seconds}"
            value="${rule.override_seconds ?? ''}">
          <div class="override-meta">
            base: ${escapeHtml(String(rule.default_seconds))} s
            <br>efectivo: ${escapeHtml(String(rule.effective_seconds))} s
          </div>
        </div>
      `).join('');
      for (const input of overrideGrid.querySelectorAll('[data-override-field]')) {
        input.addEventListener('change', persistTimerOverridesFromInputs);
      }
    }

    function renderTranscripts(forceScroll = false) {
      customerTranscript.innerHTML = state.selectedMessages
        .filter((message) => message.actor === 'customer' || message.audience === 'customer' || message.actor === 'system')
        .map((message) => renderMessage(message, 'customer'))
        .join('');
      advisorTranscript.innerHTML = state.selectedMessages
        .filter((message) => message.actor === 'advisor' || message.audience === 'advisor' || message.actor === 'system')
        .map((message) => renderMessage(message, 'advisor'))
        .join('');
      bindInteractiveActions();
      if (forceScroll || shouldStickToBottom(customerTranscript)) {
        customerTranscript.scrollTop = customerTranscript.scrollHeight;
      }
      if (forceScroll || shouldStickToBottom(advisorTranscript)) {
        advisorTranscript.scrollTop = advisorTranscript.scrollHeight;
      }
    }

    function renderMessage(message, pane) {
      const payload = message.payload || {};
      const extraImage = payload.media_url ? `<img class="img-preview" src="${payload.media_url}" alt="media">` : '';
      const actions = renderActions(message, pane);
      return `
        <div class="msg ${message.actor}">
          <div class="meta">
            <span>${escapeHtml(message.actor)} · ${escapeHtml(message.message_kind)}</span>
            <span class="stamp">${escapeHtml(formatBogotaTimestamp(message.created_at))}</span>
          </div>
          <div>${escapeHtml(message.body || '')}</div>
          ${extraImage}
          ${actions}
        </div>
      `;
    }

    function renderActions(message, pane) {
      const payload = message.payload || {};
      if (message.message_kind === 'buttons' && Array.isArray(payload.buttons)) {
        return `<div class="actions">${payload.buttons.map((button) => `
          <button class="ghost action-button" data-pane="${pane}" data-kind="button" data-id="${button.reply.id}">${escapeHtml(button.reply.title)}</button>
        `).join('')}</div>`;
      }
      if (message.message_kind === 'list' && Array.isArray(payload.sections)) {
        const rows = payload.sections.flatMap((section) => section.rows || []);
        return `<div class="actions">${rows.map((row) => `
          <button class="ghost action-button" data-pane="${pane}" data-kind="list" data-id="${row.id}">${escapeHtml(row.title)}</button>
        `).join('')}</div>`;
      }
      return '';
    }

    function renderDbPanel() {
      dbTabs.innerHTML = DB_TABLES.map((table) => `
        <button class="db-tab ${table.key === state.activeDbTab ? 'active' : ''}" type="button" data-db-tab="${table.key}">
          ${escapeHtml(table.label)}
        </button>
      `).join('');
      for (const button of dbTabs.querySelectorAll('[data-db-tab]')) {
        button.addEventListener('click', async () => {
          const tab = button.dataset.dbTab;
          if (!tab || tab === state.activeDbTab) return;
          await refreshDbTable(tab);
        });
      }

      const definition = DB_TABLES.find((table) => table.key === state.activeDbTab) || DB_TABLES[0];
      const rows = state.dbRows[definition.key] || [];
      dbMeta.textContent = state.dbGeneratedAt
        ? `actualizado ${formatBogotaTimestamp(state.dbGeneratedAt)} · ${rows.length} filas`
        : 'sin datos cargados';

      if (!rows.length) {
        dbTableWrap.innerHTML = '<div class="db-empty">No hay filas todavía para esta tabla.</div>';
        return;
      }

      dbTableWrap.innerHTML = `
        <table class="db-table">
          <thead>
            <tr>${definition.columns.map((column) => `<th>${escapeHtml(column)}</th>`).join('')}</tr>
          </thead>
          <tbody>
            ${rows.map((row) => `
              <tr>
                ${definition.columns.map((column) => `<td>${escapeHtml(formatDbCell(row[column]))}</td>`).join('')}
              </tr>
            `).join('')}
          </tbody>
        </table>
      `;
    }

    async function persistTimerOverridesFromInputs() {
      const payload = {};
      for (const field of TIMER_FIELDS) {
        payload[field.field] = null;
      }
      for (const input of overrideGrid.querySelectorAll('[data-override-field]')) {
        const value = input.value.trim();
        payload[input.dataset.overrideField] = value ? Number(value) : null;
      }
      const response = await fetchJson('/simulator/api/timer-overrides', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      state.timerRules = response.rules || [];
      renderTimerOverrides();
      if (state.selectedSnapshot) {
        renderTimerList();
      }
    }

    function bindInteractiveActions() {
      for (const button of document.querySelectorAll('.action-button')) {
        button.addEventListener('click', async () => {
          if (!state.selectedSessionId) return;
          const pane = button.dataset.pane;
          const kind = button.dataset.kind;
          const actor = pane === 'advisor' ? 'advisor' : 'customer';
          await fetch(`/simulator/api/sessions/${state.selectedSessionId}/${actor}/${kind}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ id: button.dataset.id }),
          });
          await hardRefresh(true);
        });
      }
    }

    async function sendText(actor) {
      if (!state.selectedSessionId) return;
      const input = actor === 'customer' ? document.getElementById('customer-text') : document.getElementById('advisor-text');
      const body = input.value.trim();
      if (!body) return;
      await fetch(`/simulator/api/sessions/${state.selectedSessionId}/${actor}/text`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ body }),
      });
      input.value = '';
      await hardRefresh(true);
    }

    async function sendCustomerImage() {
      if (!state.selectedSessionId) return;
      const fileInput = document.getElementById('customer-image');
      if (!fileInput.files.length) return;
      const formData = new FormData();
      formData.append('file', fileInput.files[0]);
      await fetch(`/simulator/api/sessions/${state.selectedSessionId}/customer/image`, {
        method: 'POST',
        body: formData,
      });
      fileInput.value = '';
      await hardRefresh(true);
    }

    function shouldStickToBottom(container) {
      return container.scrollHeight - container.scrollTop - container.clientHeight < 72;
    }

    function formatDbCell(value) {
      if (value === null || value === undefined || value === '') return '(vacío)';
      if (typeof value === 'object') {
        return JSON.stringify(value, null, 2);
      }
      return String(value);
    }

    function escapeHtml(value) {
      return String(value)
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;');
    }

    function lookupTimerField(key) {
      return TIMER_FIELDS.find((item) => item.key === key)?.field || '';
    }

    function formatBogotaTimestamp(value) {
      if (!value) return '(vacío)';
      return bogotaFormatter.format(new Date(value));
    }

    function formatCountdown(totalSeconds) {
      const seconds = Math.max(0, Number(totalSeconds) || 0);
      const hours = Math.floor(seconds / 3600);
      const minutes = Math.floor((seconds % 3600) / 60);
      const remainder = seconds % 60;
      const hh = hours ? `${String(hours).padStart(2, '0')}:` : '';
      return `${hh}${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`;
    }

    document.getElementById('create-session-form').addEventListener('submit', async (event) => {
      event.preventDefault();
      const customer_phone = document.getElementById('new-phone').value.trim();
      const profile_name = document.getElementById('new-name').value.trim();
      const session = await fetchJson('/simulator/api/sessions', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ customer_phone, profile_name: profile_name || null }),
      });
      document.getElementById('new-phone').value = '';
      document.getElementById('new-name').value = '';
      state.selectedSessionId = session.id;
      await hardRefresh(true);
    });
    document.getElementById('refresh-sessions').addEventListener('click', async () => {
      await hardRefresh();
    });
    document.getElementById('refresh-db').addEventListener('click', async () => {
      await refreshDbTable();
    });
    document.getElementById('reset-overrides').addEventListener('click', async () => {
      const payload = {};
      for (const field of TIMER_FIELDS) {
        payload[field.field] = null;
      }
      const response = await fetchJson('/simulator/api/timer-overrides', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      state.timerRules = response.rules || [];
      renderTimerOverrides();
    });
    document.getElementById('send-customer-text').addEventListener('click', () => sendText('customer'));
    document.getElementById('send-advisor-text').addEventListener('click', () => sendText('advisor'));
    document.getElementById('send-customer-image').addEventListener('click', sendCustomerImage);

    setInterval(() => renderTimerList(), 1000);
    setInterval(() => {
      withRefreshLock('selected_session', async () => {
        await refreshSelectedSession();
      }).catch(() => {});
    }, 2000);
    setInterval(() => {
      withRefreshLock('sessions', async () => {
        await refreshSessionsList();
      }).catch(() => {});
      withRefreshLock('db', async () => {
        await refreshDbTable();
      }).catch(() => {});
    }, 5000);

    Promise.all([
      loadTimerRules(),
      refreshSessionsList(),
      refreshDbTable(),
    ]).then(async () => {
      await refreshSelectedSession(true);
    }).catch(() => {});
  </script>
</body>
</html>
"#
    .to_string()
}
