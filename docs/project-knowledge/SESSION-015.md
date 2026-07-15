# SESSION-015

## Executive Summary

Everything the previous session claimed to have finished was independently checked line by line against the running system — and it held up. With that confidence, this session carried the AI-powered ordering assistant across the finish line: every remaining pre-launch safety check was executed for real, one real defect was caught and fixed along the way (the menu photo Meta would have shown customers had silently stopped working), the daily spending guardrails were tightened, and the assistant was switched on for the real Trabix WhatsApp number for the first time ever. From this point forward, real customers writing to the real number may be talking to the AI assistant instead of the classic scripted bot. The trial period ("canary") has begun.

## Objectives Achieved

1. ✅ Independently verified the prior session's production-readiness claims against the live system rather than trusting the write-up — everything checked out
2. ✅ Backed up the live customer database before touching anything, so a full restore point exists
3. ✅ Confirmed, with a real message sent through Meta's system, that the messaging credentials actually work
4. ✅ Audited the WhatsApp Business setup end to end (permanent access credentials, live webhook connection, verified business name, message quality standing) and caught a real problem: the menu photo customers see had expired in Meta's system, and the master image file was too large to ever be accepted — both were fixed and a working photo is live again
5. ✅ Deployed the refreshed system to production first with the AI assistant still off, confirmed the classic bot answered normally, then turned the AI assistant on
6. ✅ Tightened the AI spending safety net: lowered how many AI-assisted replies any single phone number can trigger per day, and set a hard ceiling on total daily AI usage across all customers combined
7. ✅ The AI-powered ordering assistant is now live on the real Trabix number — the trial period has started

## Business Problems Solved

### 1. A Silently Broken Menu Photo Would Have Undermined Customer Trust
**Problem:** The photo of the menu that the bot shows customers had expired inside Meta's systems — a routine but easy-to-miss expiration. Had this gone live unnoticed, customers asking to see the menu would have hit a broken image with no error message anyone would see, right at the moment they're deciding whether to order.

**Solution:** Caught during the pre-launch check, not after launch. Found that the original master photo file was also far too large for WhatsApp to ever accept (a technical size ceiling), which explains why any future attempt to refresh it the same way would have failed too. Produced a properly sized version and published it, confirmed live in Meta's system before deploying.

**Impact:** Customers see a working, professional menu photo from the moment the assistant launches — a broken first impression was avoided entirely.

### 2. Trusting Last Session's "Done" List Without Checking Would Have Been Risky
**Problem:** The previous session's closing report claimed a long list of safety work was complete. Reports can drift from reality, especially after several sessions of rapid changes — accepting the claims at face value before flipping the switch on the real customer number would have been a gamble.

**Solution:** Every claim was checked directly against the running code and a full automated test pass, not re-derived from the report's own wording. All of it held up — nothing was overstated.

**Impact:** The decision to go live was made on verified fact, not on trust in a summary.

### 3. No Safety Margin Existed Yet Between "Should Work" and "Actually Live"
**Problem:** Before this session, the AI assistant had only ever run in a private testing environment. Every check that proves it's safe for the public — a real message sent through the real messaging system, the live business account's settings, a real deploy with the safety switch still off — had never actually been executed.

**Solution:** Ran the full launch checklist in order: backed up customer data, sent and confirmed a real test message, audited the live WhatsApp Business account, deployed with the AI assistant off and confirmed the classic bot still worked, then and only then turned the AI assistant on.

**Impact:** The switch to AI was made with a proven fallback, a verified backup, and no step skipped or assumed.

## New Capabilities

- The AI-powered ordering assistant now runs live for real customers on the real Trabix WhatsApp number, alongside the always-available classic bot as an instant fallback
- A refreshed, correctly sized menu photo, confirmed working end-to-end in the real ordering flow

## Business Benefits

- **Real customers can now be served by the AI assistant** — natural conversation, order-taking, and delivery-zone handling are live for the public, not just internal testing
- **A tighter daily spending ceiling** protects against runaway AI costs while the system is still new to real-world traffic: any single customer is capped well below what a full order conversation would ever need, and total daily spend across every customer combined is bounded to a known, small number
- **A working menu photo** removes a silent point of customer friction that would otherwise have gone unnoticed until a customer complained

## Before vs After

**Before:** The AI assistant existed and had passed every test available in a private simulator, but had never handled a single real customer message. The menu photo customers would see was quietly broken. No live-system checks had been run.

**After:** The AI assistant is answering real customers on the real number. The menu photo works. Every safety check — data backup, live message delivery, business account health, safe fallback confirmed before switch-over — has been run for real, not simulated.

## Decisions

1. **Go live now, in trial mode.** Rather than wait longer, the assistant was switched on for a supervised trial period, with the classic bot one setting away as an instant fallback.
2. **Daily AI usage ceilings, set deliberately conservative for the trial.** Any one customer's conversations are capped well below what completing an order would ever require; total spend across all customers combined is capped at roughly what a handful of real trial customers ordering multiple times a day would use.
3. **The rollback rehearsal is intentionally being held off.** Confirming that switching back to the classic bot works cleanly is planned, but was deliberately not done yet — this is being sequenced deliberately rather than rushed through in one sitting.

## Rejected Alternatives

- **Trusting the previous session's report without re-verification** was considered and rejected — for a switch this consequential (a real customer-facing number), independent verification against the live system was worth the time.
- **Launching without re-checking the live WhatsApp Business account settings** (on the assumption "it already works, nothing changed") was rejected — this is exactly the kind of check that's cheap to run and expensive to skip, and it did in fact catch a real problem.

## Value Generated

The AI ordering assistant has moved from "proven in private testing" to "proven, and now live for real customers" — the single biggest milestone remaining in this initiative. A customer-facing defect (the broken menu photo) was caught and fixed before a single real customer could hit it. A full, restorable backup of customer data exists as of this session. And the system now runs with cost-safety limits tuned specifically for a careful, supervised trial rather than the earlier, looser defaults.

## Features Added

- Live AI ordering assistant on the real Trabix WhatsApp number (previously simulator-only)
- Refreshed, working menu photo
- Tighter, trial-appropriate daily AI usage ceilings (per-customer and store-wide)

## Future Opportunities

- Run the rollback rehearsal (confirm the classic bot returns cleanly if the AI assistant is switched off) at a convenient moment during the trial
- During the trial: place real test orders and watch them arrive live in the internal staff dashboard, to confirm the two systems agree on every order
- Once the trial period proves out, formally close this initiative out with a version release, and revisit two deferred decisions: where the staff dashboard should live permanently, and when to retire the older, pre-AI messaging pathway that still exists as a legacy fallback
