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
NEXT SESSION — DEAD-CODE PURGE + REMOVE THE SIMULATOR (Samuel's directive, 2026-07-20)
================================================================================

GOAL: the only code that stays is code that has a real function in the system AND actually runs
in production. Everything else goes. First fully understand how the system works end to end, then
cut aggressively so the codebase is smaller and easier to optimize.

Scope (to be executed next session, not done yet):

1. REMOVE THE SIMULATOR ENTIRELY. We don't need it. Delete everything related to it:
   - `BOT_MODE=simulator` runtime and the whole mode split (bot becomes production-only).
   - `src/simulator/` (local sessions/transcripts/media persistence), the simulator route mount,
     and the simulator branch of outbound transport (recording instead of sending).
   - `scripts/run_simulator.sh`, the simulator UI/assets, simulator timer-override plumbing.
   - All simulator references in docs (CLAUDE.md, current_runtime_reference.md, runbook.md,
     README/testing notes) and any simulator-only config.
   - After removal there is no local-chat harness — validation is via `cargo test` + the
     deterministic-engine fallback + live testing. Accept that tradeoff (Samuel confirmed).

2. REMOVE THE KNOWN DEAD HELPER: the unused `calculate_order_with_delivery` super-tool in
   `src/ai/tools.rs` (never wired into dispatch).

3. FULL DEAD-CODE SWEEP: hunt down and delete every unused path — functions, branches, config
   fields, messages, states, tools, DB columns/queries never read, `#[allow(dead_code)]` cover-ups,
   leftover FASE-x scaffolding. Anything not reachable/executed in the production agent + advisor
   flow. Understand each before deleting (don't remove something that only looks unused but is
   reached via a timer, webhook, or restore path).

4. Keep `cargo test` green throughout; document what was removed and why. Consider a version bump
   + release/tag once the codebase is trimmed.

CAUTION for whoever does this: the deterministic engine is the instant rollback for the agent, so
do NOT delete the deterministic flow even though agent mode is live — it must keep working. Only
the SIMULATOR goes, not the deterministic ENGINE. Verify each candidate is truly unreachable
(timers, boot restore, sweep, webhook verify, advisor routing) before cutting.

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
