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

- `src/routes/`: webhook verification and inbound WhatsApp handling.
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
- Phase 4 is implemented in code and pending full end-to-end WhatsApp validation:
  - real advisor routing in `webhook.rs`, including per-client advisor buttons and active advisor session binding
  - advisor detail flow: confirmation, delivery-cost capture, total final update, and closure back to `MainMenu`
  - advisor hour negotiation for detail and scheduled orders
  - 2-minute advisor timeout with `Programar`, `Reintentar`, and `Menú`
  - schedule resume path that keeps items, payment, and address while reusing `SelectDate` / `SelectTime`
  - wholesale relay mode with 30-minute inactivity timeout and advisor-side finish button
  - `Hablar con Asesor` path with advisor attend/unavailable flow plus leave-message fallback
  - timer restoration after restart for `wait_advisor_response`, `wait_advisor_mayor`, `wait_advisor_contact`, and `relay_mode`
- Phase 4 validation should now be run against `general_info/phase_planning/phase4validation.md`.

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

Operational notes:

- Live tests rely on `.env`. They now load it via `dotenvy`, but still require valid credentials and reachable services.
- `TRANSFER_PAYMENT_TEXT` should be quoted in `.env` if it contains spaces.
- `MENU_IMAGE_MEDIA_ID` must contain a valid Meta `media_id`; the runtime no longer expects separate media IDs for liquor/non-liquor flavor flows.
- `FORCE_BOGOTA_NOW=YYYY-MM-DD HH:MM` is available only for local testing of after-hours scheduling. Do not enable it in Railway or production.
- Keep `ADVISOR_PHONE` different from `WHATSAPP_TEST_RECIPIENT` during local WhatsApp validation, otherwise tester messages are routed as advisor messages.
- Large menu images may be rejected by Meta. If needed, compress or resize locally before upload.
- Operational menu images should not live under `src/` as tracked source code; the bot only needs the resulting `media_id`.

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
- validate behavior from the tester phone, not just transport
- check PostgreSQL state when the acceptance criteria require persistence evidence
- use `general_info/phase_planning/phase4validation.md` as the current checklist for advisor, timeout, relay, and restart scenarios

## Commit & Pull Request Guidelines
The current history is minimal and uses a plain descriptive subject (`setting AGENTS.MD and secuencial phase of the project`). Keep future commit subjects short, imperative, and specific. Prefer patterns like `docs: refine implementation phases` or `feat: scaffold webhook routes` over vague multi-topic messages.

Pull requests should include:

- a short summary of the change,
- affected documents or modules,
- linked issue or requirement section when available,
- screenshots or rendered Mermaid output when a flow diagram changes.
