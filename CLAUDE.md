# Trabix Granizados Bot (`granizado-bot`)

> **Lee `ROADMAP.md` al iniciar sesión.** Este archivo describe cómo está construido el bot;
> `ROADMAP.md` describe qué falta, en qué orden y qué está bloqueado. Los dos juntos son suficientes
> para trabajar sin salir del repo.

## Stack

Rust (edition 2021) · Axum · SQLx/PostgreSQL · Tokio · Meta WhatsApp Cloud API (HMAC-validated
webhooks) · Docker → Railway.

## Purpose

State-machine WhatsApp bot that takes granizado orders for Trabix Granizados, with
referral/embajador code support. Production-only: the sole runtime is the real Meta webhook
runtime (there is no simulator — it was removed in v1.8.0).

**Production runs the agent engine — always, there is no toggle.** A Claude tool-calling engine
(`src/ai/`, model `claude-sonnet-4-5` — see `DEFAULT_MODEL` in `src/ai/client.rs`) drives the
customer self-service states; `ANTHROPIC_API_KEY` is required to boot. Pricing/zones/referrals stay
deterministic via tools. Guards, cost budget, failure degradation, and the relay reachability audit
are documented in `general_info/current_runtime_reference.md` and `general_info/runbook.md`.

`BOT_ENGINE` was removed in v1.10.0 — the code no longer reads it. What remains of the original
non-LLM state machine is **legacy and unreachable in production**; do not build new behavior on it.
The removal plan (dead FSM files and what must be kept) is in
`docs/CLEANUP_deterministic_engine.md`.

Prompt caching **is implemented** (v1.9.0): the static `SYSTEM_PROMPT` carries a
`cache_control: ephemeral` breakpoint, which also caches the tool schemas; the dynamic "ESTADO
ACTUAL DEL CASO" block is sent uncached after it. `AnthropicClient::send_message` logs
`cache_read_input_tokens`/`cache_creation_input_tokens` on every call. Original spec:
`../docs/PENDIENTE_prompt_caching.md`.

## Source of truth by concern

- **Runtime behavior, timers, persistence, validation checklist** →
  `general_info/current_runtime_reference.md` (must stay aligned with this file and the diagrams
  below whenever runtime behavior changes).
- **Architecture/flow diagrams** → `general_info/complex_diagram.md` (detailed) and
  `general_info/simple_diagram.md` (simplified).
- **Version history** → `CHANGELOG.md` (Keep a Changelog + SemVer). Don't duplicate release notes
  here.
- **Licensing** → `LICENSE` — proprietary, `All Rights Reserved`.

## Code layout

- `src/routes/` — webhook verification (`verify.rs`), inbound webhook (`webhook.rs`), public
  legal pages for Meta review (`legal.rs`), and the internal outbound endpoint for `crm-app`
  (`internal.rs`, `POST /internal/advisor/send` — shared-secret header, disabled unless
  `INTERNAL_API_TOKEN` is set; contract in `docs/internal_advisor_send.md`).
- `src/engine.rs` — shared inbound-processing/outbound-action path used by webhook and timers.
- `src/whatsapp/` — Meta Cloud API client (also the `AppState.transport`), button/list builders,
  payload types.
- `src/bot/` — state machine and per-state handlers (`pricing.rs`, `states/*.rs`, `timers.rs`).
- `src/db/` — SQLx models and conversation queries.
- `config/messages.toml` — customer-facing copy, loaded at startup (restart after editing).
- `config/referrals.toml` — embajador referral codes (`codes`, `boost_codes`), loaded at startup;
  keep entries trimmed lowercase, no spaces, ≤15 chars; every `boost_codes` entry must also exist
  in `codes`. Restart after editing.
- `migrations/` — PostgreSQL schema, append-only (see below).
- `tests/` — integration tests plus `live_whatsapp.rs` (real Meta smoke test, `--ignored`).
- `crm-web/` — read-only Next.js conversation console (own `package.json`, not compiled into the
  Rust binary) reading the same PostgreSQL DB directly via `pg`. Telegram-style two-pane UI over
  the `message_events` trace: each customer is a chat; bubbles are actor-coded (client/bot/advisor)
  with lane chips separating the customer↔bot and internal bot↔advisor lanes. Single-password gate
  (`CRM_PASSWORD`). Deployed 24/7 as the `crm` Railway service (manual deploy:
  `railway up --service crm`, `rootDirectory=crm-web`, `PORT=3000`) at
  `https://crm-production-618e.up.railway.app`. Ops notes in `general_info/runbook.md`.

## Build / run / test

- `cargo check` / `cargo test` — verify + run coverage before any commit. Since the simulator was
  removed there is no local-chat harness; validation is `cargo test` + live testing on the real
  number.
- `cargo run --bin granizado-bot` — run the bot locally (needs the real env vars).
- `cargo test --test live_whatsapp -- --ignored --test-threads=1` — live WhatsApp smoke test,
  requires real credentials in `.env`.
- `cargo run --bin upload_media -- /path/to/menu.jpg` — upload local media to Meta, prints
  `media_id`.

## Operational essentials

- Docker/Railway builds must copy `config/` before `cargo build --release` — it is compiled in
  via `include_str!`.
- Production webhook path is exactly `/webhook`. The Meta app must be in `Live` mode and the WABA
  subscribed to it (`GET /{WABA_ID}/subscribed_apps`) or inbound traffic never reaches Railway even
  if webhook test events work.
- PostgreSQL sessions run on `America/Bogota` (UTC-5) so `NOW()` and stored timestamps stay aligned.
- Keep `ADVISOR_PHONE` different from `WHATSAPP_TEST_RECIPIENT` during live testing, or tester
  messages get routed as advisor messages.

## Migration safety

Never edit a migration that may already have run in Railway or any shared Postgres instance —
always add a new numbered migration instead. Editing an applied migration crashes startup with a
SQLx checksum error (`VersionMismatch(n)`). After adding a migration, update model, queries,
runtime logic, and tests together.

## Shared model with the sibling app

This bot shares one conceptual model (embajador codes, commissions, wholesale tiers, boost) with
the sibling `accountability_app` project — keep business-rule changes consistent across both when
instructed to.

## Commit conventions (overrides the generic `commit` skill for this repo)

This repo commits directly to `master` — there is no feature-branch/PR workflow and no issue
tracker (no GH/Sentry/Linear refs). When using the `commit` skill here:

- Skip its "create a feature branch before committing to master" step — direct-to-`master` is the
  established practice.
- Skip the issue-reference footer (`Fixes GH-1234` etc.) — nothing to reference.
- Use lowercase conventional types matching existing history: `feat:`, `fix:`, `docs:`, `test:`,
  `refactor:`, `chore:` (releases use `chore: release vX.Y.Z`).
- Still keep the skill's CHANGELOG.md upkeep and no-AI-authorship-marker rules — those apply as-is.
- Bump `Cargo.toml`/`Cargo.lock` together with `CHANGELOG.md` on every release commit; tag releases
  with `git tag -a vX.Y.Z -m "Release vX.Y.Z"`.
