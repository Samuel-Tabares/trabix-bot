# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

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
