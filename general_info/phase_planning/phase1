  # Phase 1 Plan

  ## Summary

  Build Phase 1 as a deployable Rust service at the repo root that:

  - loads validated config from env,
  - exposes GET /webhook and POST /webhook,
  - validates Meta HMAC signatures,
  - acknowledges valid webhooks immediately with 200,
  - processes webhook payloads asynchronously,
  - sends text, buttons, lists, image, and mark-as-read requests through a WhatsAppClient,
  - provisions PostgreSQL migrations and basic conversation CRUD,
  - is deployable to Railway with the Dockerfile from the design spec.

  ## Implementation Changes

  1. Scaffold the Rust project in the current repo root, not a nested folder.
      - Create Cargo.toml, .gitignore, .env.example, src/, and migrations/.
      - Keep package and binary name granizado-bot.
      - Use the spec stack with minimal required features: tokio/full, sqlx with postgres, runtime-tokio, chrono, json, macros, migrate, reqwest with json and TLS support, serde
        with derive.
  2. Add configuration and startup.
      - Implement src/config.rs with Config::from_env() -> Result<Config, ConfigError>.
      - Load .env in development with dotenvy.
      - Required env vars: the 7 vars from the design spec; PORT defaults to 8080.
      - src/main.rs must initialize tracing, create the DB pool, run sqlx::migrate!(), build AppState { config, pool, wa_client }, register routes, and bind 0.0.0.0:PORT.
  3. Add WhatsApp payload types and client.
      - Create src/whatsapp/mod.rs, types.rs, client.rs, buttons.rs.
      - Implement typed serde models for inbound: text, button_reply, list_reply, image.
      - Implement typed serde models for outbound: text, button, list, image, mark-as-read.
      - Correct the builder API so it is usable: quick_buttons(to, body, buttons) and quick_list(to, body, button_text, rows).
      - Add unit tests for inbound text/button/list/image deserialization and outbound text/button/list/image/read serialization.
      - Because the design doc lacks a list_reply sample, include one local fixture test that matches Meta’s schema.
  4. Add DB layer and migrations.
      - Create src/db/mod.rs, models.rs, queries.rs.
      - Create the 3 SQL migrations exactly from the design doc tables.
      - Implement init_pool() plus the Phase 1 CRUD/query functions from the implementation guide.
      - Store state as VARCHAR, with app-side serialization to snake_case strings.
  5. Add routes and webhook processing.
      - GET /webhook: verify hub.verify_token and return hub.challenge on success, 403 otherwise.
      - POST /webhook: read raw bytes, validate X-Hub-Signature-256, return 401 if invalid or missing.
      - For valid signatures, return 200 OK immediately and spawn async processing with cloned app state and raw body bytes.
      - Async processing flow: deserialize payload, ignore payloads without messages, extract the first message, log sender/type/content, call mark_as_read(), and send an echo
        text reply.
      - For Phase 1, do not run the state machine yet and do not use DB conversation state in message handling beyond pool startup/migration validation.
  6. Add container/deploy setup and manual external checkpoints.
      - Create the multi-stage Dockerfile exactly from the design spec.
      - Plan deployment steps for GitHub, Railway service creation, PostgreSQL attachment, env var setup, and Meta webhook verification using the Railway URL.
      - Use the Meta test number for Phase 1 smoke tests; do not plan real-number migration yet.

  ## Test Plan

  - cargo check, cargo test, and cargo build --release all pass locally.
  - Config tests cover complete env and missing-variable errors.
  - HMAC unit tests cover valid signature, missing header, malformed header, and invalid signature.
  - Serde tests cover inbound text/button/list/image and outbound text/button/list/image/read payloads.
  - Local smoke tests:
      - GET /webhook returns the challenge with the correct token and 403 otherwise.
      - Invalid signed POST /webhook returns 401.
      - Valid signed POST /webhook returns 200 immediately and the async handler logs processing.
  - DB smoke tests: migrations apply to PostgreSQL and basic create_conversation / get_conversation / update_state work.
  - External smoke tests after deploy: Meta verifies the webhook, test number receives send_text, send_buttons, and send_list.

  ## Assumptions

  - Phase 1 implements send_image() transport and tests, but live menu media IDs are not required yet.
  - The main interactive list button text defaults to Ver opciones unless a flow explicitly overrides it.
  - The design guide corrections above are part of the execution spec for Phase 1.
  - Railway/Meta actions are included as manual checkpoints, not automated scripts.