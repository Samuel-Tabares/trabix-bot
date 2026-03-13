# Repository Guidelines

## Project Structure & Module Organization
This repository started as documentation-first, but it now contains a working Rust service plus the original planning documents.

Current source-of-truth areas:

- `AGENTS.md`: contributor instructions for this repository.
- `general_info/Business_Requirements_Document.md`: business rules, operations, costs, and phased scope.
- `general_info/Flow_Design_Diagram.mermaid`: end-to-end conversation flow.
- `general_info/Software_Design_Document.md`: target Rust architecture, data model, state machine, and webhook contract.
- `general_info/Implementation_&_Deployment_Document.md`: phased build plan and acceptance criteria.
- `general_info/phase_planning/phase1.md`: approved Phase 1 implementation plan.
- `general_info/phase_planning/phase1validation.md`: Phase 1 validation notes.
- `general_info/phase_planning/phase2.md`: approved Phase 2 implementation plan.
- `general_info/phase_planning/phase2validation.md`: Phase 2 validation checklist and evidence guide.
- `general_info/phase_planning/phase3.md`: approved Phase 3 planning baseline.
- `general_info/phase_planning/phase3validation.md`: Phase 3 validation checklist, evidence guide, and manual test setup.
- `general_info/phase_planning/phase4.md`: approved Phase 4 implementation plan.
- `general_info/phase_planning/phase4validation.md`: Phase 4 validation checklist, evidence guide, manual relay/advisor scenarios, and restart checks.

Current code layout:

- `src/routes/`: webhook verification, inbound WhatsApp handling, and public legal pages for Meta review.
- `src/whatsapp/`: Meta Cloud API client, button/list builders, and payload types.
- `src/bot/`: state machine and per-state handlers.
- `src/db/`: SQLx models and conversation queries.
- `src/bin/`: local operational utilities such as media upload to Meta.
- `migrations/`: PostgreSQL schema.
- `tests/`: local integration and live smoke tests.

Current implementation status:

- Phase 1 is implemented and validated: webhook, HMAC verification, WhatsApp client, PostgreSQL, migrations.
- Phase 2 is implemented and validated as the current runtime flow:
  - persistent conversation state machine
  - main menu, view menu, schedules
  - immediate vs scheduled delivery
  - customer data collection
  - item loop until `ShowSummary`
  - persistence in PostgreSQL across messages and restarts
  - flexible text capture for programmed date/time with minimal length validation
  - flavor selection through WhatsApp lists after choosing `Con Licor` or `Sin Licor`
  - single menu image sent only in `Ver Menú`
- Phase 3 is implemented and partially validated as the checkout foundation:
  - real price calculation in `pricing.rs`, including liquor pair promo and wholesale tiers
  - `ShowSummary` with estimated total excluding delivery cost
  - payment choice: `Contra Entrega` or `Pago Ahora`
  - transfer instructions plus receipt image capture
  - 10-minute receipt timer with timeout options to change payment or cancel
  - address confirmation/editing before handoff
  - order draft/final persistence in `orders` and `order_items`
  - conversation persistence of payment context, receipt state, and timer rehydration data
  - handoff persistence into `pending_advisor`
- Phase 4 is implemented and now working as the current production runtime:
  - real advisor routing in `webhook.rs`, including per-client advisor buttons and active advisor session binding
  - advisor detail flow: confirmation, delivery-cost capture, total final update, and closure back to `MainMenu`
  - advisor hour negotiation for detail and scheduled orders
  - 2-minute advisor timeout with `Programar`, `Reintentar`, and `Menú`
  - schedule resume path that keeps items, payment, and address while reusing `SelectDate` / `SelectTime`
  - wholesale relay mode with 30-minute inactivity timeout and advisor-side finish button
  - `Hablar con Asesor` path with advisor attend/unavailable flow plus leave-message fallback
  - timer restoration after restart for `wait_advisor_response`, `wait_advisor_mayor`, `wait_advisor_contact`, and `relay_mode`
  - production number receiving real public WhatsApp messages once the Meta app is in `Live` mode and subscribed to the WABA
  - public legal endpoints for Meta review: `/privacy-policy` and `/terms-of-service`
- Phase 4 validation remains documented in `general_info/phase_planning/phase4validation.md`, but the runtime is now proven against real inbound and outbound production traffic.

Current real runtime behavior:

- Any public user can message the production WhatsApp number when the Meta app is in `Live` mode and the WABA is correctly subscribed to the app.
- Messages from `ADVISOR_PHONE` are never treated as customer messages; they always enter the advisor flow.
- If the advisor writes without first selecting a pending case, the bot should answer with the advisor guidance message rather than the customer menu.
- The bot replies with text, buttons, lists, and images through Meta Cloud API, and persists conversation/order state in PostgreSQL.
- `mark_as_read` is best-effort only. If Meta rejects the read receipt request, the bot logs a warning and continues processing the message.

## Build, Test, and Development Commands
Use these commands regularly:

- `git status --short --branch`: confirm working tree state before editing.
- `rg --files`: list repository files quickly.
- `sed -n '1,120p' general_info/Business_Requirements_Document.md`: inspect a document section before editing.
- `cargo check`: verify the Rust project compiles.
- `cargo test`: run local unit and integration coverage.
- `cargo test --test phase2_flow`: run the Phase 2 end-to-end state-machine flow.
- `cargo test --test live_db -- --ignored --test-threads=1`: run live PostgreSQL smoke tests.
- `cargo test --test live_whatsapp -- --ignored --test-threads=1`: run live WhatsApp transport smoke tests.
- `cargo run --bin granizado-bot`: run the local bot service.
- `cargo run --bin upload_media -- /ruta/local/menu.jpg`: upload a local media file to Meta and print the `media_id`.
- `curl -H "Authorization: Bearer $WHATSAPP_TOKEN" "https://graph.facebook.com/v21.0/$WABA_ID/phone_numbers"`: confirm which phone numbers the current token can operate.
- `curl -H "Authorization: Bearer $WHATSAPP_TOKEN" "https://graph.facebook.com/v21.0/$WABA_ID/subscribed_apps"`: confirm the WABA is subscribed to the current Meta app.
- `curl -X POST -H "Authorization: Bearer $WHATSAPP_TOKEN" "https://graph.facebook.com/v21.0/$WABA_ID/subscribed_apps"`: subscribe the app to the WABA when real inbound events are not reaching the webhook.

Operational notes:

- Live tests rely on `.env`. They now load it via `dotenvy`, but still require valid credentials and reachable services.
- Customer-facing bot copy now lives in `config/messages.toml` and is loaded at startup; restart the service after editing that file.
- `TRANSFER_PAYMENT_TEXT` is now optional fallback-only in `.env` for backward compatibility if `config/messages.toml` leaves `checkout.transfer_payment_text` empty.
- `MENU_IMAGE_MEDIA_ID` must contain a valid Meta `media_id`; the runtime no longer expects separate media IDs for liquor/non-liquor flavor flows.
- `FORCE_BOGOTA_NOW=YYYY-MM-DD HH:MM` is available only for local testing of after-hours scheduling. Do not enable it in Railway or production.
- Keep `ADVISOR_PHONE` different from `WHATSAPP_TEST_RECIPIENT` during local WhatsApp validation, otherwise tester messages are routed as advisor messages.
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
This repository now uses release versions and tags.

- Stable release line:
  - `v1` / `v1.0.0`: baseline project state before the post-release workflow fixes
  - `v1.1` / `v1.1.0`: workflow bugfix release
  - `v1.1.1`: migration-checksum hotfix for Railway deployment safety
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
- `tests/phase2_flow.rs` covers the current customer flow, including persistence/rehydration between steps and list-based flavor selection.
- `tests/live_db.rs` covers live database persistence and migrations.
- `tests/live_whatsapp.rs` covers live transport to Meta for text, buttons, and lists.

When adding Phase 3+ work, prefer tests named by behavior, for example `applies_liquor_pair_discount` or `handles_transfer_payment_without_receipt`.

Current important coverage areas:

- `src/bot/pricing.rs`: detail promo, wholesale pricing, mixed-order totals.
- `src/bot/states/checkout.rs`: payment selection, receipt handling, address confirmation, and handoff entry.
- `src/bot/states/advisor.rs`: advisor button parsing, delivery-cost capture, hour negotiation, timeout retry/programming, and contact-advisor flow.
- `src/bot/states/relay.rs`: wholesale relay, advisor-contact relay, finish button, and text-only forwarding rules.
- `src/bot/timers.rs`: receipt timeout, advisor timeout, relay inactivity timeout, and timer restoration after restart.

For manual WhatsApp validation:

- confirm the webhook points to the active service
- confirm Meta is using the exact `/webhook` callback URL
- confirm the app is in `Live` mode when testing with non-tester public numbers
- confirm `GET /{WABA_ID}/subscribed_apps` returns the active app before investigating missing inbound logs
- validate behavior from the tester phone, not just transport
- validate behavior from a non-advisor public number when confirming open production access
- do not use `ADVISOR_PHONE` to validate the customer flow; it is routed through advisor logic by design
- check PostgreSQL state when the acceptance criteria require persistence evidence
- use `general_info/phase_planning/phase4validation.md` as the current checklist for advisor, timeout, relay, and restart scenarios

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
