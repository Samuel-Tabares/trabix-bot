# SESSION-014

## Executive Summary

Executed the production-launch roadmap from SESSION-013: the AI-powered bot can now legally run on the real WhatsApp number (the safety lock that restricted it to local testing was removed), and every protection the roadmap demanded before facing the public was built and verified — a safety net so no customer is ever left in silence if the AI provider goes down, hard spending limits so a stranger can't run up the AI bill, duplicate-message protection, and defenses against customers trying to trick the assistant into fake discounts. Live rehearsals in the local simulator then caught two serious behavioral defects that no automated test had found — in both, the assistant *said* it had done something it never actually did. Both were fixed with deterministic safeguards (not just better instructions) and the failing scenario was re-run until a real order completed perfectly, with correct totals and correct business analytics. What remains is exclusively the go-live itself: Meta checklist, the canary trial with real orders, and cost measurement — all requiring Samuel.

## Objectives Achieved

1. ✅ Removed the simulator-only lock: the AI engine now boots in production mode (and leaving the switch unset still means the classic bot — rollback stays a one-minute operation)
2. ✅ Built the failure safety net: if the AI provider fails, the customer immediately gets a fixed "we're having a technical issue" message and the advisor gets the full case context — never silence
3. ✅ Built cost controls: 60 AI calls per customer per day, an optional global daily kill-switch, a memory window so long-time customers don't cost more per message, and oversized messages are trimmed
4. ✅ Hardened against abuse: duplicate WhatsApp deliveries are ignored, and the assistant's instructions now explicitly refuse customer attempts to change prices, invent discounts, or impersonate the advisor
5. ✅ Audited and documented that the legacy "relay" chat mode is unreachable while the AI engine drives (it stays functional only for conversations started under the classic bot)
6. ✅ Caught and fixed two real behavioral defects through live simulator rehearsals (details below)
7. ✅ Full test suite green (152 unit + 3 database integration tests, zero warnings); operations runbook written; all reference documentation updated

## Business Problems Solved

### 1. If the AI Provider Goes Down, Customers Would Have Heard Nothing
**Problem:** Any failure in the AI service (outage, timeout, exhausted credit) was only written to an internal log. The customer's message would simply never be answered — the worst possible experience for a business built on WhatsApp responsiveness.

**Solution:** Every AI failure now triggers two immediate messages: the customer gets a warm "technical problem, an advisor will contact you" note, and the advisor gets the customer's name, number, last message, and where the order stood. The conversation freezes in place, so when service recovers, the customer just writes again and everything resumes.

**Impact:** The advisor is the alarm system. No lost customers during outages.

### 2. A Stranger Could Have Run Up the AI Bill Indefinitely
**Problem:** Every incoming message triggered AI processing with no ceiling — a prankster (or a bug) sending a thousand messages would pay Trabix's AI bill a thousand times over. Worse, each customer's full conversation history was resent to the AI on every message, so costs grew forever for loyal customers.

**Solution:** Each phone number gets 60 AI calls per day (after that: fixed message + advisor alert, once). An optional global daily limit can cap total spend. The AI now only reads the recent portion of long conversations — full history is still stored for the business dashboard, it just isn't re-billed every message.

**Impact:** The worst-case daily AI spend is now a known, bounded number.

### 3. The Assistant Claimed Success for Actions It Never Took (Two Defects)
**Problem — found only by rehearsing like a real customer:** In one rehearsal the assistant told the customer their total was $28.000 when the real total was $18.000 — it invented the number instead of using the calculator. In another, it told the customer "your order is confirmed and on its way" when it had never actually registered the order in the system: no order record existed, the advisor's "yes I can deliver" had nowhere to go, and the sale would have silently evaporated while the customer waited for a delivery that was never coming.

**Solution:** Instructions alone proved insufficient — the assistant ignored even explicit corrective hints. The fix is structural: every money figure the assistant might repeat is now handed to it verbatim by the calculation tools; the advisor's reply is always routable to the right case the moment the assistant contacts the advisor; and when the advisor confirms an order that was never formally registered, the system now registers it automatically — the exact failure scenario was re-run and completed flawlessly (correct order record, correct $22.000 total quoted at every step, analytics recorded).

**Impact:** The two highest-risk failure modes for real customers — wrong prices and phantom confirmations — now have deterministic guardrails, not promises.

## Key Lesson Learned

**The AI sometimes narrates instead of acting — and testing must compare what it *says* against what the *database* shows.** All 152 automated tests were green while both behavioral defects existed. They were only visible by driving a full conversation and then checking the order records. This transcript-versus-database comparison is now a core checklist item for the canary phase.

Also: a leftover bot process from a previous work session was still occupying the local port, so the first hour of rehearsals unknowingly ran against two-day-old code. Always verify what's actually running before testing.

## Current Status

- **Code:** production-ready, pending the canary trial. Rollback = remove one environment variable.
- **Documentation:** operations runbook (`general_info/runbook.md`), updated runtime reference, updated CHANGELOG.
- **Not done (requires Samuel):** Railway database backup, live WhatsApp smoke test, Meta account checklist, canary deployment with 2–3 real testers, cost-per-conversation measurement, decision on the staff dashboard hosting, and the formal version release once the canary passes.

## Decisions Needed From Samuel

1. Acceptable daily AI budget (value for the global kill-switch variable)
2. Who tests during the canary and for how long (suggested: 2–3 people, 2–3 days)
3. Staff dashboard (crm-web): host it on Railway behind a password, or keep it local-only
4. When to flip the switch — first day should be with the advisor available
