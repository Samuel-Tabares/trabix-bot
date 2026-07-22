# Trabix Granizados Bot

Trabix is a real Rust WhatsApp ordering bot for granizados, based in Armenia, Quindío, Colombia.
It is a production-only service: the single runtime is the real Meta/WhatsApp Cloud API webhook
runtime (HMAC-validated). There is no local simulator — it was removed in v1.8.0.

It reuses one core runtime for:

- conversation state machine
- PostgreSQL persistence
- order and order-item storage
- advisor flow
- timers and timeout recovery
- pricing and checkout behavior

## Engines

Two engines are selected via `BOT_ENGINE` (see `general_info/current_runtime_reference.md`):

- `deterministic` (default when the variable is unset): the original non-LLM state machine.
  Removing `BOT_ENGINE` in Railway and redeploying is the instant rollback path.
- `agent`: the Claude Haiku tool-calling engine in `src/ai/` for customer self-service states.
  Requires `ANTHROPIC_API_KEY`. Pricing, delivery zones, and referrals stay deterministic via tools.

## Run

```bash
cargo run --bin granizado-bot
```

Copy `.env.example` to `.env` and fill in the real Meta credentials, `DATABASE_URL`, and
`ADVISOR_PHONE`. The server binds to `0.0.0.0:$PORT` (default 8080) and serves the webhook at
`/webhook`.

## Test

```bash
cargo check
cargo test
```

Since there is no simulator, validation is `cargo test` + the deterministic-engine fallback +
live testing on the real WhatsApp number. The live smoke test needs real credentials:

```bash
cargo test --test live_whatsapp -- --ignored --test-threads=1
```

DB-backed integration tests (`agent_degradation`, `customer_analytics`) are `#[ignore]` and need
`TEST_DATABASE_URL` pointing at a reachable local Postgres.

## Deploy

Deployed on Railway via `Dockerfile`. The image copies `config/` (compiled in via `include_str!`)
and `migrations/`. The Meta app must be in `Live` mode and the WABA subscribed to it, or inbound
traffic never reaches Railway. PostgreSQL sessions run on `America/Bogota` (UTC-5).

## Upload menu media

```bash
cargo run --bin upload_media -- /path/to/menu.jpg
```

Prints the Meta `media_id` to set as `MENU_IMAGE_MEDIA_ID`.

## Conversation console (`crm-web/`)

Read-only Next.js console over the bot's `message_events` trace, styled like a messaging app:
the left rail lists each customer as a chat (search, last message, recency); the right pane
replays the full thread with actor-coded bubbles (client / bot / advisor) and lane chips that
separate the customer↔bot conversation from the internal bot↔advisor orchestration. Polls the DB
for near-live updates. Gated by a single shared password (`CRM_PASSWORD`).

**Production:** deployed 24/7 as the `crm` service in the bot's Railway project, reading the same
Postgres over the private network — `https://crm-production-618e.up.railway.app`. Deploys are
manual (`railway up --service crm --detach` from the repo root; the service is configured with
`rootDirectory=crm-web` and `PORT=3000`). Ops details in `general_info/runbook.md`.

**Local dev:**

```bash
cd crm-web
npm install   # only if node_modules isn't already there
npm run dev
```

Open `http://localhost:3000`. Set `DATABASE_URL` and `CRM_PASSWORD` in `crm-web/.env.local`
(see `crm-web/.env.example`) — point `DATABASE_URL` at the bot's Postgres (Railway public URL or
local) to see real data.
