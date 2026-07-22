# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0] - 2026-07-22

### Changed
- **Reworked into a conversation console (Telegram/WhatsApp style).** Two-pane layout: a left rail
  lists each customer as a "chat" (avatar, name/phone, last-message preview, relative time, message
  count) sorted by most recent activity with live search; the right pane replays the full thread as
  actor-coded bubbles — customer (left, neutral), bot (right, purple), advisor (amber, own lane) —
  with day dividers and "Cliente ⇄ Bot" / "Bot ⇄ Asesor" lane chips so the internal advisor
  orchestration is visible alongside the customer conversation. Reads the bot's new `message_events`
  table; polls for near-live updates. Falls back to an empty state if the table doesn't exist yet.
- Data layer trimmed to the console queries (`listCases`, `getCaseTimeline`, `getCaseHeader`); the
  old customer-table/detail/order/referral UI and its API routes were removed.

### Added
- Single-operator auth gate: one shared password (`CRM_PASSWORD`) via a login page + hashed,
  httpOnly session cookie, enforced by a `proxy.ts` (Next 16) gate over all routes except login.

## [0.1.0] - 2026-07-13

### Added
- Initial CRM dashboard: customer list with search (name/phone/username) and sorting by spend, units purchased, or last contact.
- Customer detail page with three tabs: conversation transcript (parsed from `agent_case_messages`), order history (with items), and referral-code usage.
- Connects directly to the bot's PostgreSQL database via `pg` — no data replication, no Supabase involved.
