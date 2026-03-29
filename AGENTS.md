# Repository Guidelines

## Project Structure & Module Organization
This repository started as documentation-first, but it now contains a working Rust service plus current runtime reference documents.

Current source-of-truth areas:

- `AGENTS.md`: contributor instructions for this repository.
- `README.md`: public project overview and local simulator quickstart.
- `general_info/complex_diagram.mermaid`: detailed end-to-end runtime and architecture flow.
- `general_info/simple_diagram.mermaid`: simplified customer/advisor flow diagram.
- `general_info/current_runtime_reference.md`: current runtime behavior, operational constraints, persistence, timers, and validation checklist.
- `LICENSE`: proprietary repository distribution terms with `All Rights Reserved` and limited evaluation-only simulator permission.

Current code layout:

- `src/routes/`: webhook verification, inbound WhatsApp handling, local simulator routes/UI, and public legal pages for Meta review.
- `src/whatsapp/`: Meta Cloud API client, button/list builders, and payload types.
- `src/bot/`: state machine and per-state handlers.
- `src/db/`: SQLx models and conversation queries.
- `src/engine.rs`: shared inbound-processing and outbound-action execution path used by webhook, simulator, and timers.
- `src/simulator/`: local simulator persistence helpers for sessions, transcripts, and local media.
- `src/transport.rs`: outbound transport selection between Meta and simulator recording.
- `scripts/`: local launch helpers, including cross-platform simulator startup wrappers.
- `src/bin/`: local operational utilities such as media upload to Meta.
- `migrations/`: PostgreSQL schema.
- `tests/`: local integration and live smoke tests.

Current implementation status:

- Phase 1 is implemented and validated: webhook, HMAC verification, WhatsApp client, PostgreSQL, migrations.
- Phase 2 is implemented and validated as the current runtime flow:
  - persistent conversation state machine
  - main menu with welcome hours plus buttons for `Hacer Pedido`, `Ver Menú`, and `Hablar con Asesor`
  - view menu and delivery scheduling
  - immediate vs scheduled delivery
  - customer data auto-prefill from inbound WhatsApp metadata plus ask-only-missing capture
  - item loop until `ShowSummary`, with partial-summary buttons for `Agregar más`, `Finalizar pedido`, and confirmed `Reiniciar pedido`
  - persistence in PostgreSQL across messages and restarts
  - flexible text capture for programmed date/time with minimal length validation
  - flavor selection through WhatsApp lists after choosing `Con Licor` or `Sin Licor`
  - single menu image sent only in `Ver Menú`
- Phase 3 is implemented and validated as the checkout foundation:
  - real price calculation in `pricing.rs`, including liquor pair promo and wholesale tiers
  - `ShowSummary` with estimated total excluding delivery cost
  - payment choice: `Contra Entrega` or `Pago Ahora`, plus `Cancelar Pedido`
  - transfer instructions plus receipt image capture
  - 10-minute receipt timer with timeout options to change payment or cancel
  - customer-data review/edit step before handoff, covering name, phone, and address
  - order draft/final persistence in `orders` and `order_items`
  - conversation persistence of payment context, receipt state, and timer rehydration data
  - handoff persistence into `pending_advisor`
- Phase 4 is implemented and now working as the current production runtime:
  - real advisor routing in `webhook.rs`, including per-client advisor buttons and active advisor session binding
  - advisor detail flow: confirmation, delivery-cost capture, total final update, and closure back to `MainMenu`
  - advisor hour negotiation for detail and scheduled orders
  - 2-minute advisor timeout with `Programar`, `Reintentar`, and `Menú`
  - 30-minute hard reset for advisor-managed `ask_delivery_cost`, `negotiate_hour`, `wait_advisor_hour_decision`, and `wait_advisor_confirm_hour`, with order status moved to `manual_followup`
  - generic client inactivity handling on customer-input states: one reminder at 2 minutes and reset to `MainMenu` after 35 minutes, excluding advisor/receipt/relay timed waits
  - schedule resume path that keeps items, payment, and address while reusing `SelectDate` / `SelectTime`
  - wholesale relay mode with 30-minute inactivity timeout and advisor-side finish button
  - `Hablar con Asesor` path with advisor attend/unavailable flow plus leave-message fallback
  - timer restoration after restart for advisor waits, stuck advisor-detail waits, `relay_mode`, and generic customer inactivity
  - periodic database-backed timer sweep so missed in-memory tasks still expire receipt, advisor, relay, and customer inactivity waits
  - production number receiving real public WhatsApp messages once the Meta app is in `Live` mode and subscribed to the WABA
  - public legal endpoints for Meta review: `/privacy-policy` and `/terms-of-service`
- Phase 5 is implemented and validated as the current local manual-testing runtime:
  - `BOT_MODE=simulator` runs the same bot brain without Meta credentials
  - local `/simulator` web UI with customer and advisor panes
  - simulated customers identified by phone plus optional profile name
  - reuse of `conversations`, `orders`, `order_items`, and timer behavior for faithful local persistence
  - simulator transcript and local media persistence in PostgreSQL plus filesystem-backed uploads
  - per-message `America/Bogota` timestamps in the simulator transcript
  - active timer inspector with countdowns, deadlines, and timeout phase/state visibility
  - simulator-only timer overrides from the UI for faster local timeout validation
  - simulator system notices when a timeout comes from runtime expiry, periodic sweep, or boot reconciliation
  - cross-platform simulator launch helpers for macOS/Linux and Windows, with optional Docker-based local Postgres bootstrap
- The old phase-planning documents were removed because they no longer matched the live system. Use `general_info/current_runtime_reference.md` for the current runtime and validation reference.

Current real runtime behavior:

- Any public user can message the production WhatsApp number when the Meta app is in `Live` mode and the WABA is correctly subscribed to the app.
- Messages from `ADVISOR_PHONE` are never treated as customer messages; they always enter the advisor flow.
- If the advisor writes without first selecting a pending case, the bot should answer with the advisor guidance message rather than the customer menu.
- The bot replies with text, buttons, lists, and images through Meta Cloud API, and persists conversation/order state in PostgreSQL.
- For customer messages, the runtime seeds `customer_phone` from inbound WhatsApp `from` and seeds `customer_name` from `contacts[].profile.name` when Meta includes it; manual edits remain authoritative.
- Backend logs now prioritize operational flow visibility: masked phone suffixes, short content previews, state transitions, outbound action summaries, and timer recovery/expiry events. Meta status-only webhooks should stay out of `INFO` noise unless `DEBUG` is enabled.
- `mark_as_read` is best-effort only. If Meta rejects the read receipt request, the bot logs a warning and continues processing the message.
- Generic customer inactivity is only armed by a real inbound customer message. After the 35-minute inactivity reset sends its notice and returns the conversation to `main_menu`, no new inactivity reminder/reset should fire until the customer writes again.
- On deploy/restart, overdue timers are reconciled silently in persistence. The bot should not fan out timeout or inactivity WhatsApp messages just because the process booted; only still-active timers are re-armed for future expiration.
- `BOT_MODE=simulator` is local-only transport mode. It uses the same runtime flow and persistence but records outbound messages in the simulator transcript instead of sending anything to Meta.
- In simulator mode, local testing data must stay in the local environment: local database, local uploads directory, and local Axum UI only.
- Production and simulator must stay aligned for all shared bot-brain behavior: state machine transitions, timers, persistence, pricing, order flow, advisor flow, and customer-facing outcomes. If production logic changes one of those areas, simulator behavior must change too. Only Meta/webhook transport concerns may intentionally differ.

## Build, Test, and Development Commands
Use these commands regularly:

- `git status --short --branch`: confirm working tree state before editing.
- `rg --files`: list repository files quickly.
- `sed -n '1,120p' general_info/Business_Requirements_Document.md`: inspect a document section before editing.
- `cargo check`: verify the Rust project compiles.
- `cargo test`: run local unit and integration coverage.
- `cargo test --test live_whatsapp -- --ignored --test-threads=1`: run live WhatsApp transport smoke tests.
- `cargo run --bin granizado-bot`: run the local bot service.
- `BOT_MODE=simulator cargo run --bin granizado-bot`: run the full local simulator without Meta.
- `./scripts/run_simulator.sh`: launch the simulator on macOS/Linux with sensible defaults and optional Docker-backed local Postgres.
- `scripts\\run_simulator.bat`: launch the simulator on Windows through the PowerShell wrapper.
- `cargo run --bin upload_media -- /ruta/local/menu.jpg`: upload a local media file to Meta and print the `media_id`.
- `curl -H "Authorization: Bearer $WHATSAPP_TOKEN" "https://graph.facebook.com/v21.0/$WABA_ID/phone_numbers"`: confirm which phone numbers the current token can operate.
- `curl -H "Authorization: Bearer $WHATSAPP_TOKEN" "https://graph.facebook.com/v21.0/$WABA_ID/subscribed_apps"`: confirm the WABA is subscribed to the current Meta app.
- `curl -X POST -H "Authorization: Bearer $WHATSAPP_TOKEN" "https://graph.facebook.com/v21.0/$WABA_ID/subscribed_apps"`: subscribe the app to the WABA when real inbound events are not reaching the webhook.

Operational notes:

- Live tests rely on `.env`. They now load it via `dotenvy`, but still require valid credentials and reachable services.
- Docker/Railway builds must copy both `assets/` and `config/` into the builder stage before `cargo build --release`, because simulator UI files and `config/messages.toml` are compiled in via `include_str!`.
- `BOT_MODE=production` is still the default. Omit `BOT_MODE` in Railway unless you explicitly mean to run simulator mode.
- `BOT_MODE=simulator` does not require WhatsApp credentials and should bind to `127.0.0.1` by default.
- The simulator always uses the tracked file `assets/trabix-menu.png` for `Ver Menú`. Replace that tracked file if the shared simulator menu image changes in the future.
- `SIMULATOR_UPLOAD_DIR` stores local receipt/image uploads for simulator conversations and should stay outside production deploy flows.
- Customer-facing bot copy now lives in `config/messages.toml` and is loaded at startup; restart the service after editing that file.
- `TRANSFER_PAYMENT_TEXT` is now optional fallback-only in `.env` for backward compatibility if `config/messages.toml` leaves `checkout.transfer_payment_text` empty.
- `MENU_IMAGE_MEDIA_ID` must contain a valid Meta `media_id`; the runtime no longer expects separate media IDs for liquor/non-liquor flavor flows.
- `FORCE_BOGOTA_NOW=YYYY-MM-DD HH:MM` is available only for local testing of after-hours scheduling. Do not enable it in Railway or production.
- PostgreSQL sessions opened by the app are set to `America/Bogota` (`UTC-5`) on connect so SQL `NOW()` usage and timestamp display stay aligned with the operating timezone used by the bot.
- Keep `ADVISOR_PHONE` different from `WHATSAPP_TEST_RECIPIENT` during local WhatsApp validation, otherwise tester messages are routed as advisor messages.
- In simulator mode, no local message or media should ever be sent to Meta. If simulator testing appears in WhatsApp, treat it as a configuration bug.
- Large menu images may be rejected by Meta. If needed, compress or resize locally before upload.
- Operational menu images should not live under `src/` as tracked source code; the bot only needs the resulting `media_id`.
- The production webhook callback URL is `/webhook`. Do not configure `/webhooks`, trailing variants, or alternate paths in Meta.
- For public production traffic, the Meta app must be in `Live` mode. `Development` mode restricts inbound traffic to approved testers only.
- A verified webhook and a connected production number are not sufficient by themselves. The WABA must also be subscribed to the app through `/{WABA_ID}/subscribed_apps`, or real user messages may never reach Railway even if Meta webhook test events work.
- The token used by Railway must be a permanent system-user token with access to the same WABA and `WHATSAPP_PHONE_ID` used by the production number.
- `WHATSAPP_TEST_RECIPIENT` is only for live smoke tests and does not control which number the bot listens on. Runtime inbound traffic is determined by Meta's WABA, app subscription, and `WHATSAPP_PHONE_ID`.
- If sending works but no real inbound message appears in Railway logs, first verify `GET /{WABA_ID}/subscribed_apps` before debugging Rust code.
- Meta review/live approval now depends on the public legal pages served by this app: `/privacy-policy` and `/terms-of-service`.

## Traceability & Change Discipline
Every meaningful code change should leave a clear trace in the repository history.

- Start every work session with `git status --short --branch` and review unexpected local changes before editing.
- Keep commits focused. Do not mix runtime fixes, docs cleanup, refactors, and release metadata in the same commit unless they are inseparable.
- Before committing, inspect the exact diff with `git diff` and ensure unrelated files are not included by accident.
- If a change affects runtime behavior, persistence, pricing, timers, or WhatsApp flow, add or update tests in the same work cycle.
- If a change affects shared bot behavior, update production and simulator together. Do not land production-only logic in shared engine/state/timer paths unless the simulator is updated to match the resulting customer/advisor behavior.
- If a change affects customer-facing copy, note whether it came from `config/messages.toml`, `.env` fallback behavior, or hardcoded runtime text.
- If a bug is found in production, document the root cause in the commit message or changelog entry, not only in chat or deployment logs.

## Database & Migration Safety
This project uses SQLx migrations at startup. Migration discipline is mandatory.

- Never edit the contents of an existing migration that may already have run in Railway or any shared PostgreSQL instance.
- To change schema after deployment, always add a new migration file with the next numeric prefix.
- Editing an already-applied migration can crash startup with a SQLx checksum error such as `VersionMismatch(n)`.
- If a migration adds new columns needed by fresh installs, keep the original table-creation migration unchanged once deployed and add the new columns in a later migration as well.
- Before pushing schema changes, verify local startup still works with `cargo check` and `cargo test`.
- After adding a migration, confirm the code paths that read and write the new fields are updated together: model, queries, runtime logic, and tests.

## Versioning & Releases
This repository now uses release versions and tags, every change made on the project is named and versioned so we can keep a full audit, every change made is also updated on these 3 files: AGENTS.md ; CHANGELOG.md ; the current source-of-truth Mermaid flow diagram (`general_info/complex_diagram.mermaid` when the runtime flow changes).

- Stable release line:
  - `v1` / `v1.0.0`: baseline project state before the post-release workflow fixes
  - `v1.1` / `v1.1.0`: workflow bugfix release
  - `v1.1.1`: migration-checksum hotfix for Railway deployment safety
  - `v1.1.2`: timer-sweep hotfix for missed in-memory expirations
  - `v1.2.0`: main-menu simplification, `America/Bogota` SQL session timezone, generic customer inactivity handling, and 30-minute hard reset for stuck advisor-detail waits
  - `v1.3.0`: full local simulator mode with shared engine/transport, persisted transcripts/media, and Axum-served customer/advisor chat UI
  - `v1.4.0`: simulator timer observability with timestamps, countdown/debug panel, UI overrides, and timeout source notices
  - `v1.4.1`: repository licensing metadata with proprietary `All Rights Reserved` terms and evaluation-only simulator permission
  - `v1.4.2`: cross-platform simulator launcher scripts and tracked fallback menu asset
  - `v1.4.3`: fixed simulator menu asset path using the tracked fallback file only
  - `v1.4.4`: real tracked simulator menu image replaces the placeholder asset
  - `v1.4.5`: top-level README onboarding for the project and local simulator quickstart
  - `v1.4.6`: finalize README onboarding and remove an accidentally tracked local simulator upload artifact
  - `v1.5.0`: simulator UI refresh with session-centric advisor chat, auto-refresh, and raw database inspector tabs
  - `v1.5.1`: simulator HTTP handlers moved under `src/simulator/` and frontend assets extracted into `assets/simulator/`
  - `v1.5.2`: repository diagram-source rename plus Docker/Railway build fix for compile-time simulator assets and message config
- Use semantic versioning from this point forward:
  - `MAJOR` for breaking changes or major product resets
  - `MINOR` for backward-compatible feature releases
  - `PATCH` for backward-compatible bugfixes and deployment hotfixes
- Update `Cargo.toml`, `Cargo.lock`, and `CHANGELOG.md` together for every release commit.
- Release commits should use a dedicated message such as `chore: release v1.1.1`.
- Create annotated tags for releases, for example `git tag -a v1.1.1 -m "Release v1.1.1"`.
- Do not create a release tag for unvalidated code. Run the required checks first.

## Push & Deploy Workflow
Use this sequence whenever a change is meant to go live.

- 1. Review the tree with `git status --short` and confirm only intended files are staged.
- 2. Run validation at minimum with `cargo check` and `cargo test`.
- 3. Commit the functional change.
- 4. If this is a release, bump version files and create a dedicated release commit.
- 5. Tag the release with an annotated tag.
- 6. Push branch and tags explicitly:
  - `git push origin master`
  - `git push origin <tag>`
- 7. Redeploy Railway only after the push succeeds and the intended commit is visible on GitHub.
- 8. If Railway crashes on startup after a deploy, inspect migrations first before assuming the Rust logic is wrong.
- Never push unrelated local deletions or experimental files together with a production hotfix.

## Coding Style & Naming Conventions
Keep Markdown concise, task-oriented, and aligned with the existing Spanish project documents. Use clear section headings and short paragraphs. For Mermaid, prefer readable node labels and grouped flow sections.

For Rust code, follow standard conventions: 4-space indentation, `snake_case` for files/functions/modules, `PascalCase` for structs/enums, and small focused modules such as `pricing.rs` or `state_machine.rs`. Avoid `unwrap()` in production code; the implementation guide explicitly requires structured error handling.

Persisted conversation state uses `snake_case` strings in the database. Any transient data required to rehydrate parameterized states must live in `conversations.state_data`.

## Testing Guidelines
Automated coverage already exists and should be extended with each phase:

- `src/` unit tests cover webhook parsing, config loading, state serialization, validations, and state transitions.
- `tests/live_whatsapp.rs` covers live transport to Meta for text, buttons, and lists.

When adding Phase 3+ work, prefer tests named by behavior, for example `applies_liquor_pair_discount` or `handles_transfer_payment_without_receipt`.

Current important coverage areas:

- `src/bot/pricing.rs`: detail promo, wholesale pricing, mixed-order totals.
- `src/bot/states/checkout.rs`: payment selection, receipt handling, customer-data review/editing, and handoff entry.
- `src/bot/states/advisor.rs`: advisor button parsing, delivery-cost capture, hour negotiation, timeout retry/programming, and contact-advisor flow.
- `src/bot/states/relay.rs`: wholesale relay, advisor-contact relay, finish button, and text-only forwarding rules.
- `src/bot/timers.rs`: receipt timeout, advisor timeout, relay inactivity timeout, and timer restoration after restart.
  - also includes the periodic database-backed sweep that catches missed expirations after deploys or runtime interruptions
- `src/engine.rs`: shared customer/advisor processing and action execution across webhook, simulator, and timers
- `src/routes/simulator.rs` and `src/simulator/`: local simulator transport, transcript persistence, and upload/media handling

For manual WhatsApp validation:

- confirm the webhook points to the active service
- confirm Meta is using the exact `/webhook` callback URL
- confirm the app is in `Live` mode when testing with non-tester public numbers
- confirm `GET /{WABA_ID}/subscribed_apps` returns the active app before investigating missing inbound logs
- validate behavior from the tester phone, not just transport
- validate behavior from a non-advisor public number when confirming open production access
- do not use `ADVISOR_PHONE` to validate the customer flow; it is routed through advisor logic by design
- check PostgreSQL state when the acceptance criteria require persistence evidence
- use `general_info/current_runtime_reference.md` as the current checklist for advisor, timeout, relay, restart, and operational validation scenarios

For manual simulator validation:

- run with `BOT_MODE=simulator`
- use a local PostgreSQL database only
- create a local customer session with phone and optional profile name
- validate that phone/name auto-prefill, manual edits, and `main_menu` resets preserve persisted customer data
- validate immediate order, scheduled order, advisor handoff, relay, timeout, and restart-recovery paths through `/simulator`
- validate receipt upload with a real local image file
- verify every transcript row shows a Bogota timestamp
- verify the timer panel shows countdown, start time, expiration time, and timeout phase/state
- use simulator timer overrides only for local validation of new waits
- confirm that no local testing event appears in WhatsApp or requires Meta credentials

## Commit & Pull Request Guidelines
The current history is minimal and uses a plain descriptive subject (`setting AGENTS.MD and secuencial phase of the project`). Keep future commit subjects short, imperative, and specific. Prefer patterns like `docs: refine implementation phases` or `feat: scaffold webhook routes` over vague multi-topic messages.

Recommended commit prefixes for this repository:

- `feat:` new customer-facing or advisor-facing behavior
- `fix:` runtime bugfix, persistence fix, webhook fix, or deploy correction
- `docs:` documentation-only changes
- `test:` test-only additions or corrections
- `refactor:` internal cleanup without intended behavior change
- `chore:` release/versioning, dependency, or operational maintenance

Pull requests should include:

- a short summary of the change,
- affected documents or modules,
- linked issue or requirement section when available,
- screenshots or rendered Mermaid output when a flow diagram changes.

For release or hotfix pull requests, also include:

- target version or tag,
- validation commands executed,
- migration impact and whether Railway/shared PostgreSQL requires new schema application,
- rollout or redeploy notes when applicable.
