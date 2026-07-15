Your checklist — in this exact order
1. Push (if Railway deploys from GitHub)


git push
If Railway deploys via CLI instead, skip and use railway up at step 5.

2. Backup the Railway DB (before anything touches production)


railway link          # select the bot project
Copy DATABASE_URL from the Railway dashboard (the public connection URL), then:


pg_dump "<DATABASE_URL>" -Fc -f trabix_backup_2026-07-15.dump
3. Live WhatsApp smoke test — put the real Meta credentials in .env (WHATSAPP_TOKEN, WHATSAPP_PHONE_ID, plus WHATSAPP_TEST_RECIPIENT — must differ from ADVISOR_PHONE), then:


cargo test --test live_whatsapp -- --ignored --test-threads=1
4. Meta checklist (5 min, dashboard) — app in Live mode · WABA subscribed (GET /{WABA_ID}/subscribed_apps lists the app) · webhook = https://<railway-domain>/webhook with correct verify token · token is a permanent system-user token · MENU_IMAGE_MEDIA_ID still valid (if expired: cargo run --bin upload_media -- <menu.jpg> and update the variable).

5. Deploy — two steps

First deploy without BOT_ENGINE set → send a real message, confirm the deterministic bot answers (migrations run automatically at boot).
Then in Railway set BOT_ENGINE=agent, ANTHROPIC_API_KEY=<secret>, and I suggest AGENT_DAILY_LLM_CALL_LIMIT=300 (≈ US$2–3/day worst case on Haiku) → redeploy. Never set FORCE_BOGOTA_NOW in Railway.
6. Canary (2–3 days, you + 2–3 conocidos ordering for real). Each day:

Read transcripts (agent_case_messages or crm-web) and compare every total the bot told a customer against orders.total_final — this comparison is what catches the bug class we found twice.
Check Anthropic console spend and Railway logs (agent turn failed, LLM daily budget exhausted).
7. Rollback drill (once, during canary): remove BOT_ENGINE → redeploy → confirm the deterministic bot answers → re-enable agent.

8. When the canary passes, come back and tell me — I'll cut the release (minor version bump + tag), and we can then decide the crm-web hosting and schedule the optimization cycle (relay removal, etc.).

The full failure playbook (silent bot, stuck case, media expired) is in runbook.md.
