# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

- Set every SQLx PostgreSQL session to `America/Bogota` so app-driven `NOW()` and timestamp display align with `UTC-5` operations.
- Remove the standalone `Horarios` menu flow, move the immediate-delivery hours into the welcome message, and switch the main menu to 3 WhatsApp buttons.
- Add a generic customer inactivity timer: resend the current prompt once after 2 minutes, then reset to `main_menu` after 35 minutes without customer activity.

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
