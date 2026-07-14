# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] - 2026-07-13

### Added
- Initial CRM dashboard: customer list with search (name/phone/username) and sorting by spend, units purchased, or last contact.
- Customer detail page with three tabs: conversation transcript (parsed from `agent_case_messages`), order history (with items), and referral-code usage.
- Connects directly to the bot's PostgreSQL database via `pg` — no data replication, no Supabase involved.
