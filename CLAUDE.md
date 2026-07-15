# Trabix Granizados Bot (`granizado-bot`)

## Stack

Rust (edition 2021) · Axum · SQLx/PostgreSQL · Tokio · Meta WhatsApp Cloud API (HMAC-validated
webhooks) · Docker → Railway.

## Purpose

State-machine WhatsApp bot that takes granizado orders for Trabix Granizados, with
referral/embajador code support. Two runtimes selected via `BOT_MODE`:

- `production` (default): real Meta webhook runtime.
- `simulator`: local web chat at `http://127.0.0.1:8080/simulator` running the same bot brain,
  no calls to Meta — launch with `./scripts/run_simulator.sh`.

Two engines selected via `BOT_ENGINE` (independent of `BOT_MODE`; both work in production):

- `deterministic` (default when unset): the original non-LLM state machine. Removing
  `BOT_ENGINE` in Railway + redeploy is the instant rollback path.
- `agent`: Claude Haiku tool-calling engine (`src/ai/`) for customer self-service states;
  requires `ANTHROPIC_API_KEY`. Pricing/zones/referrals stay deterministic via tools. Guards,
  cost budget, failure degradation, and the relay reachability audit are documented in
  `general_info/current_runtime_reference.md` and `general_info/runbook.md`.

## Source of truth by concern

- **Runtime behavior, timers, persistence, validation checklist** →
  `general_info/current_runtime_reference.md` (must stay aligned with this file and the diagrams
  below whenever runtime behavior changes).
- **Architecture/flow diagrams** → `general_info/complex_diagram.mermaid` (detailed) and
  `general_info/simple_diagram.mermaid` (simplified).
- **Version history** → `CHANGELOG.md` (Keep a Changelog + SemVer). Don't duplicate release notes
  here.
- **Licensing** → `LICENSE` — proprietary, `All Rights Reserved`, evaluation-only simulator use.

## Code layout

- `src/routes/` — webhook verification (`verify.rs`), inbound webhook (`webhook.rs`), simulator
  mount (`simulator.rs`), public legal pages for Meta review (`legal.rs`).
- `src/engine.rs` — shared inbound-processing/outbound-action path used by webhook, simulator,
  and timers. Production and simulator share this — a change here must be validated in both.
- `src/whatsapp/` — Meta Cloud API client, button/list builders, payload types.
- `src/bot/` — state machine and per-state handlers (`pricing.rs`, `states/*.rs`, `timers.rs`).
- `src/db/` — SQLx models and conversation queries.
- `src/simulator/` — local simulator persistence (sessions, transcripts, local media).
- `src/transport.rs` — outbound transport selection (Meta vs. simulator recording).
- `config/messages.toml` — customer-facing copy, loaded at startup (restart after editing).
- `config/referrals.toml` — embajador referral codes (`codes`, `boost_codes`), loaded at startup;
  keep entries trimmed lowercase, no spaces, ≤15 chars; every `boost_codes` entry must also exist
  in `codes`. Restart after editing.
- `migrations/` — PostgreSQL schema, append-only (see below).
- `tests/` — integration tests plus `live_whatsapp.rs` (real Meta smoke test, `--ignored`).

## Build / run / test

- `cargo check` / `cargo test` — verify + run coverage before any commit.
- `cargo run --bin granizado-bot` — run production-mode locally.
- `./scripts/run_simulator.sh` (or `BOT_MODE=simulator cargo run --bin granizado-bot`) — simulator
  mode, no Meta credentials needed, binds to `127.0.0.1`.
- `cargo test --test live_whatsapp -- --ignored --test-threads=1` — live WhatsApp smoke test,
  requires real credentials in `.env`.
- `cargo run --bin upload_media -- /path/to/menu.jpg` — upload local media to Meta, prints
  `media_id`.

## Operational essentials

- `BOT_MODE=production` is the default — omit it in Railway unless simulator mode is intended.
- Docker/Railway builds must copy `assets/` and `config/` before `cargo build --release` — both
  are compiled in via `include_str!`.
- Production webhook path is exactly `/webhook`. The Meta app must be in `Live` mode and the WABA
  subscribed to it (`GET /{WABA_ID}/subscribed_apps`) or inbound traffic never reaches Railway even
  if webhook test events work.
- `FORCE_BOGOTA_NOW` is local-testing only — never enable in Railway/production.
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
