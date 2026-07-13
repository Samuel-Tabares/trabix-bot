# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- Automatic capture of Meta WhatsApp username in webhook and persistent storage in `customers` table to enable customer identification by username as backup if phone changes.
- Permanent conversation memory: agent conversation history now persists indefinitely by customer instead of clearing after checkout, enabling full CRM view of all previous interactions.
- Persistent customer CRM data via new `customers` table (migration 008): tracks unique customer by `phone_number_meta` from Meta, with optional manual phone/name, username, last delivery address, and cumulative totals (spend and units). Supports cross-conversation history without limits.
- Referral code analytics via new `referral_code_analytics` table (migration 009): tracks usage count, total discounts generated, ambassador commissions, units purchased, and gross sales per code for business intelligence and commission reporting.
- Claude Haiku 4.5 AI agent mode (BOT_ENGINE=agent) for customer conversations: orchestrates menu selection, data collection (name, phone, address), order assembly, delivery-zone detection, and checkout with tool-calling. Agent handles customer/advisor message routing, confirms availability and payment method, and bridges to advisor. Deterministic pricing, delivery zones, referrals, and validation remain unchanged. Conversation locks prevent race conditions on concurrent messages from customer and advisor. Conversation memory persists agent history between turns in `agent_case_messages` table. New migration: `007_create_agent_case_messages.sql`. Configuration: `BOT_ENGINE` env var selects engine; agent mode currently enabled only with `BOT_MODE=simulator`. New files: `src/ai/{client.rs,agent.rs,tools.rs,memory.rs}`, `src/bot/delivery_zone.rs`.
- Three deterministic calculation tools for agent (FASE 2): `get_delivery_cost()` resolves delivery zones (Armenia norte/centro/sur, nearby towns, or manual unknown), `apply_referral_discount()` applies referral codes with boost detection, and `calculate_order_with_delivery()` computes complete order summary (items + delivery + referral discount + ambassador commission) in one step. All tools delegate to existing pricing and delivery-zone logic; no rule changes.

### Changed
- Main menu now shows only "Hacer Pedido" and "Ver Menú" buttons; "Hablar con Asesor" button removed from menu (agent handles advisor requests based on text input in FASE 6).
- Granizado pricing: "Segundo con licor" renamed to "Par con licor" at $12.000 (2 units at half price).
- Order summary (`render_summary()`) now displays automatic delivery cost and referral discount breakdown inline instead of deferring to advisor; includes Subtotal, Domicilio, and Total with referral discount details when applicable.

## [1.7.2] - 2026-04-30

- Restore safe advisor routing by quoted Meta `context.id` for advisor replies while keeping active advisor-session fallback when no valid quote exists.
- Keep customer webhook intake tolerant of external Meta payloads with profile names, interactive messages, and missing or empty quoted context IDs, preserving the early `received whatsapp message` log.
- Add referral boost codes from `config/referrals.toml`; boost codes must also be valid referral codes and add 5 percentage points to ambassador commission without changing the customer discount.
- Update tracked referral codes to `trabix-prueba15`, `rider332`, and `bytebann`, with `trabix-prueba15` enabled as the boosted code.
- Use a 23-hour stale cutoff for scheduled-order `ask_delivery_cost` waits while keeping immediate delivery-cost waits on the existing 30-minute hard reset.
- Store new advisor reply-thread and referral boost metadata in `conversations.state_data` with legacy JSON defaults, avoiding a SQL migration.

## [1.7.0] - 2026-04-07

- Add a wholesale-only ambassador referral step before payment with `Tengo código` / `Seguir sin código`, lowercase code validation from `config/referrals.toml`, and retry/skip handling for invalid codes.
- Apply referral discounts only to wholesale-priced buckets, persist `referral_code` plus discount/commission totals in `orders` and `state_data`, and update final customer totals without discounting delivery cost.
- Show referral discount details in the customer payment-ready summary and include referral code plus ambassador accounting totals in advisor summaries once a valid code is used.
- Round the client referral discount up to the next `$100` so values like `$4.510` become `$4.600`, then recompute the paid subtotal and ambassador commission from that rounded discount.
- Restrict tracked referral codes to trimmed lowercase strings without spaces and with a maximum length of 15 characters.
- Add a simulator-only Bogotá clock override in the local UI so immediate-hours, out-of-hours, and scheduling flows can be validated without restarting the app or setting `FORCE_BOGOTA_NOW`.

## [1.6.1] - 2026-04-06

- Send the advisor a final confirmed-order packet with customer data, order details, and final totals when the customer completes payment by `Contra Entrega` or `Pago Ahora`.

## [1.6.0] - 2026-04-06

- Replace the old `show_summary` checkout split with a combined `review_checkout` step plus a final button-based payment selection after advisor handling.
- Move advisor delivery-cost capture before final payment, auto-accept scheduled orders after delivery cost, and remove the wholesale-specific checkout relay branch.
- Change immediate-order advisor waiting so a 5-minute silence auto-falls back to the same branch as `No puedo`, while `Hablar con Asesor` now exposes only `Atender` on the advisor side.
- Remove the manual advisor `No puedo` button from `wait_advisor_response` and keep the 5-minute timeout as the only fallback into hour negotiation.
- Stop re-sending relay `Finalizar` on every forwarded client message; the advisor now receives that button only on the initial relay handoff.
- Keep receipt upload at the end of the flow, forward uploaded receipts to the advisor, and update diagrams/runtime docs to match the new production flow.

## [1.5.2] - 2026-03-29

- Rename the tracked Mermaid flow documents to `general_info/complex_diagram.mermaid` and `general_info/simple_diagram.mermaid`.
- Update repository guidance and runtime reference docs so they point to the new diagram source-of-truth files.
- Fix Docker/Railway builds by copying `assets/` and `config/` into the builder image so compile-time `include_str!` simulator UI and message-config assets exist during `cargo build --release`.
- Copy `assets/` into the runtime image as well so simulator mode can still serve the tracked `assets/trabix-menu.png` menu file from disk.

## [1.5.1] - 2026-03-26

- Refactor the simulator boundary so HTTP handlers move into `src/simulator/web.rs`, leaving `src/routes/simulator.rs` as a thin mount wrapper while keeping the shared production bot brain unchanged.
- Extract the simulator frontend into editable files under `assets/simulator/` (`index.html`, `simulator.css`, `simulator.js`) and serve them through dedicated simulator asset routes.

## [1.5.0] - 2026-03-26

- Upgrade the local simulator UI so the advisor pane is session-centric, the old advisor inbox panel is removed, and timer-driven transcript updates appear without manual page refresh.
- Add a read-only database inspector inside `/simulator` for raw `conversations`, `orders`, and `order_items` rows, plus backend list queries for those tables.

## [1.4.6] - 2026-03-26

- Finalize the public README onboarding and remove an accidentally tracked simulator upload artifact from the repository so the public tree matches the intended local-only simulator workflow.

## [1.4.5] - 2026-03-26

- Add a top-level `README.md` that explains the project, the proprietary repository terms, and how to run the real production bot brain locally through the simulator on macOS, Linux, and Windows.
- Document simulator boundaries clearly so users understand that shared bot logic is reused locally while Meta-specific transport behavior still requires real WhatsApp validation.

## [1.4.4] - 2026-03-26

- Replace the simulator placeholder menu asset with the real tracked menu image at `assets/trabix-menu.png`.
- Keep simulator menu serving fixed to the tracked repository asset so every clone sees the same menu by default.

## [1.4.3] - 2026-03-26

- Remove `SIMULATOR_MENU_IMAGE_PATH` from simulator configuration and always serve the tracked fallback asset for `Ver Menú`.
- Keep the cross-platform simulator launchers but simplify them to the fixed tracked menu asset workflow so teams can replace that file and push it with the repository.

## [1.4.2] - 2026-03-26

- Add cross-platform simulator launcher scripts for macOS/Linux (`scripts/run_simulator.sh`) and Windows (`scripts/run_simulator.ps1`, `scripts/run_simulator.bat`) that preconfigure simulator env vars and can auto-start a local Postgres container when Docker is available.
- Add a tracked placeholder menu asset for local simulator bootstrapping and support serving `.svg` menu assets in the simulator.

## [1.4.1] - 2026-03-26

- Add a top-level proprietary `LICENSE` with `All Rights Reserved` terms and a narrow evaluation-only permission to view the repository and run the local simulator for personal testing.

## [1.4.0] - 2026-03-26

- Add simulator-side timer observability with per-message Bogota timestamps, active timer panels, deadline/countdown display, and simulator-only timer speed overrides from the local UI.
- Make timer restoration and sweep logic simulator-override aware so local timeout validation follows the same shared engine path while remaining isolated from production transport.
- Record simulator timer system notices so receipt, advisor, relay, and inactivity expirations show whether they came from runtime, sweep, or boot reconciliation.

## [1.3.0] - 2026-03-26

- Add a local `BOT_MODE=simulator` runtime that serves `/simulator` and exercises the same bot state machine, PostgreSQL persistence, advisor flow, pricing, and timers without calling Meta.
- Refactor inbound processing and outbound action execution into a shared engine so webhook messages, simulator messages, and timer expirations all use the same runtime path.
- Add simulator transcript and media persistence with new PostgreSQL tables for local sessions, chat history, and uploaded receipt/image files.
- Add an Axum-served local chat UI with multi-session customer testing, advisor interaction, button/list replay, local image upload, and persisted state inspection.

## [1.2.0] - 2026-03-25

- Set every SQLx PostgreSQL session to `America/Bogota` so app-driven `NOW()` and timestamp display align with `UTC-5` operations.
- Remove the standalone `Horarios` menu flow, move the immediate-delivery hours into the welcome message, and switch the main menu to 3 WhatsApp buttons.
- Add a generic customer inactivity timer: resend the current prompt once after 2 minutes, then reset to `main_menu` after 35 minutes without customer activity.
- Add a 30-minute hard reset for stuck advisor-detail waits (`ask_delivery_cost`, `negotiate_hour`, `wait_advisor_hour_decision`, `wait_advisor_confirm_hour`) and move timed-out orders to `manual_followup`.

## [1.1.2] - 2026-03-22

- Add a periodic database-backed timer sweep so receipt, advisor, and relay expirations still fire if an in-memory task is missed.
- Keep the existing boot-time timer restoration and make timeout handling more resilient after deploys or runtime interruptions.

## [1.1.1] - 2026-03-13

- Fix Railway startup by restoring the original checksum for migration `002_create_orders.sql`.
- Keep the new order schedule text columns exclusively in migration `004_add_order_schedule_text.sql`, which is safe for existing databases and fresh installs.

## [1.1.0] - 2026-03-13

- Process every inbound WhatsApp message in batched webhook payloads instead of dropping all but the first.
- Resume timed-out wholesale scheduling through the correct advisor state.
- Preserve accepted free-form scheduled date and time values in persisted orders.
- Move the remaining receipt-timeout prompt body into message configuration.

## [1.0.0] - 2026-03-13

- Baseline release of the Rust WhatsApp ordering bot before the post-release workflow bugfixes.
- Includes the implemented customer flow, checkout foundation, and advisor/relay logic currently present in the repository state prior to the `v1.1.0` fixes.
