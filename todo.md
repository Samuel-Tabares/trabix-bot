Status as of 2026-07-15: agent engine is LIVE on the real WhatsApp number.

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
