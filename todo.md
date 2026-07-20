Status as of 2026-07-15: agent engine is LIVE on the real WhatsApp number.

2026-07-19: canary testing found real bugs, logged in docs/canary-fixes-2026-07-19.md.

2026-07-19 (later same day, see SESSION-016): worked through the list one item at a time with
confirmation before each fix. RESOLVED + tested (cargo test 177 passed): item 2 (LLM narrating
totals — added a deterministic guard that blocks any $ amount not backed by a real tool-result),
item 8 (delivery fee missing from "final" totals — now clearly labeled subtotal until delivery
is known), item 4 + finding D (scheduled orders wrongly required advisor availability
confirmation — now auto-accept correctly, transfer+scheduled flow fixed), item 5 (flavor
disambiguation — Maracumango/Manzana verde/Bonbonbum/Blueberry now require the customer's
wording to distinguish the liquor/non-liquor variant instead of guessing). NOT committed or
deployed yet — pending Samuel's decision on when to ship.

2026-07-20 (SESSION-017): closed the ENTIRE remaining canary backlog, one item at a time with
confirmation before each fix. RESOLVED + tested (cargo test 187 passed): finding A (duplicate
confirmed orders — no reset on confirm, order_confirmed guard, modify_confirmed_order /
start_new_order tools, delta analytics; confirmed with Samuel: reopen & replace same order),
item 9 (mandatory referral prompt on wholesale via finalize_checkout guard + skip_referral_code
tool; confirmed: code only applies to wholesale), item 1 (business hours + Bogotá clock injected
into the system prompt every turn), item 7 (mandatory final recap — prompt reinforcement), item 3
(fixed no-LLM welcome + timers to plain text in agent mode; confirmed both), item 6 (deterministic
**x**→*x* normalization), finding C (Meta vs. custom customer fields — no migration, customers
table already had the columns). Docs updated: CHANGELOG.md, general_info/current_runtime_reference.md,
docs/canary-fixes-2026-07-19.md. Committed locally this session; NOT pushed/deployed yet.

2026-07-20 (later): SESSION-016 + SESSION-017 pushed to production (origin/master ->
fb0a421..27aafda), Railway auto-deploys from master. Simulator validation was skipped — shipped
straight to prod per Samuel's call (and the simulator is being removed anyway, see below).

================================================================================
DONE (2026-07-19) — SIMULATOR REMOVED + DEAD-CODE PURGE (v1.8.0, not yet pushed)
================================================================================

Executed Samuel's directive. Mapped the system end to end first, then cut. `cargo test` green
throughout (180 passed, 0 failed, 4 ignored; 0 build warnings). Cargo.toml bumped 1.7.2 -> 1.8.0.

What was removed:
1. THE SIMULATOR, entirely. Deleted `src/simulator/`, `src/routes/simulator.rs`, `src/transport.rs`,
   `assets/`, `scripts/run_simulator.*`. Dropped `BOT_MODE`/`BotMode`/`SimulatorConfig`/`mode`; the
   bot is production-only. `OutboundTransport` enum deleted — `AppState.transport` is now the
   `WhatsAppClient` directly. `ProductionConfig` flattened into `Config`. Removed the simulator-only
   timer machinery in `bot/timers.rs` (`SimulatorTimerOverrides`, `TimerOverridesHandle`,
   `simulator_timer_rules`, `simulator_timer_snapshots`, `record_simulator_timer_notice`, all the
   `is_simulator` threading). KEY INVARIANT: production always used `default_duration()` already
   (overrides only applied under `is_simulator`), so timing is unchanged. Removed the simulator
   `session_phone`/`file_path` plumbing from the engine send path (kept `session_phone` where it
   doubles as advisor-reply-thread target — that's real production logic). `agent_degradation.rs`
   rewritten to assert the DB-observable invariant (no error propagation + state unchanged).
2. `FORCE_BOGOTA_NOW` + the simulator clock override — `now_bogota()` is now always `Utc::now()`.
3. Dead `calculate_order_with_delivery` super-tool + `OrderSummary` (never wired into dispatch).
4. Dead `TimerSource::BootReconcile` variant (only fed the removed simulator notices).

Docs updated: CLAUDE.md, README.md, .env.example, .gitignore, general_info/current_runtime_reference.md,
general_info/complex_diagram.md, AI_AGENT_FAQ.md, CHANGELOG.md, Dockerfile (dropped `COPY assets/`).

Left in place on purpose:
- Migration `005_create_simulator_tables.sql` (append-only history; its tables are now orphaned but
  harmless — Samuel chose NOT to add a drop migration).
- The deterministic ENGINE (the agent's instant rollback — must keep working).
- `#![allow(dead_code)]` on `src/db/{models,queries}.rs` and `whatsapp/buttons.rs`: pre-existing,
  covers `FromRow` structs whose fields are used by SQLx deserialization, not simulator-related.
  Risky to prune without per-field analysis — deferred as a possible future sweep.
- MASTER_PROMPT.md / MASTER_PROMPT_PRODUCCION.md: historical rollout-planning docs (reference an
  already-removed gate); left as point-in-time records.

NOT pushed/deployed yet — pending Samuel's decision on when to ship (and whether to tag v1.8.0).

Done:
1. ✅ Pushed to GitHub
2. ✅ Backed up Railway DB (local, gitignored: backups/trabix_backup_2026-07-15.dump)
3. ✅ Live WhatsApp smoke test passed
4. ✅ Meta checklist verified via Graph API: permanent system-user token, webhook subscribed
   and active, display name verified, quality GREEN. MENU_IMAGE_MEDIA_ID had expired (11MB
   PNG also exceeded Meta's 5MB limit) — recompressed to 276KB JPEG, re-uploaded, new ID
   deployed: 1322542426631050.
5. ✅ Deploy step 1 (BOT_ENGINE unset) — confirmed deterministic bot replied normally.
6. ✅ Deploy step 2 — BOT_ENGINE=agent, ANTHROPIC_API_KEY, AGENT_DAILY_LLM_CALL_LIMIT=120 set
   and deployed clean. Per-phone daily limit lowered 60→30 (code change, pushed, tested).

What's left — in this order:

1. Canary (2–3 days, you + 2–3 conocidos ordering for real). Each day:
   - Read transcripts (agent_case_messages or crm-web) and compare every total the bot told
     a customer against orders.total_final — this comparison is what catches the bug class
     we found twice before (hallucinated totals, phantom confirmations).
   - Check Anthropic console spend and Railway logs (agent turn failed, LLM daily budget
     exhausted).
   - Current budget: 30 LLM calls/phone/day, 120 total/day across everyone (~4 full orders).

2. Rollback drill (once, during canary, whenever you're ready — not done yet on purpose):
   remove BOT_ENGINE → redeploy → confirm the deterministic bot answers → re-enable agent.

3. When the canary passes, come back and I'll cut the release (minor version bump + tag),
   and we can then decide crm-web hosting and schedule the optimization cycle (relay
   removal, etc.).

Also queued: watch crm-web live while you text the bot, next time you're testing.

The full failure playbook (silent bot, stuck case, media expired) is in runbook.md.
