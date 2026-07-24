# SESSION-019

## Executive Summary

This session started from a real test the founder ran on the live WhatsApp ordering assistant, plus the internal conversation console that lets the team read past chats end to end. We logged into that console, replayed the full test order the founder had placed, and used it as evidence to find everything the assistant was doing wrong — then fixed it.

The test order surfaced a set of concrete, money-relevant problems: the assistant miscounted a customer's order (said "45 drinks" while it had actually registered 35), it sent customers rapid-fire bursts of two or three separate messages in under a second and occasionally contradicted itself between them, it left old tappable buttons appearing in a flow that is supposed to be pure natural conversation, and — most importantly — when it couldn't reach a human advisor in time it told the customer to "start over from the menu" even though the order had actually been saved. It also mishandled scheduled orders and dates, didn't enforce the real product availability rules, and never showed the advisor the discount-code details needed to pay ambassadors.

We addressed all of it. The assistant now runs on a meaningfully more capable reasoning engine, sends one clean message per turn instead of a burst, always tells the customer the true order count, keeps the "talk-to-advisor" wait window at a more forgiving length while reassuring the customer their order is safe, understands and validates scheduling dates with a firm 24-hour minimum, enforces the current "alcohol-free is sold wholesale only" stock reality, adds the new tamarind flavor, and shows advisors the referral code, the customer discount, and the ambassador's commission whenever a code is used. We also did a full sweep to confirm that, in the live AI mode, no tappable buttons ever reach customers anywhere. Everything was verified against the full automated test suite before being recorded and published.

## Objectives Achieved

1. ✅ Logged into the internal conversation console and reviewed the founder's real test order end to end, using it as the evidence base for the whole session
2. ✅ Upgraded the assistant's underlying reasoning engine to a substantially more capable one, targeting the counting, date, and product-confusion errors at the root
3. ✅ Eliminated the "burst of messages" behavior — the assistant now replies with a single, coherent message per turn
4. ✅ Guaranteed the order count the assistant states always matches what it actually registered
5. ✅ Made the alcohol-free rule real: those flavors are sold wholesale-only right now, enforced automatically, with an easy switch to turn retail back on when stock returns
6. ✅ Added the new tamarind (with-alcohol) flavor to the menu
7. ✅ Gave advisors full visibility of any discount code used: which code, the customer's discount, and the ambassador's commission
8. ✅ Reworked scheduled orders so the assistant genuinely understands the requested date and time and enforces a 24-hour minimum lead time
9. ✅ Made the advisor wait window more forgiving and, when it lapses, reassures the customer their order is saved instead of telling them to start over
10. ✅ Swept the entire system to confirm that in live AI mode no tappable buttons reach customers, and documented why the button machinery still exists (it powers the instant fallback to the older rule-based assistant)

## Business Problems Solved

- **The assistant was quoting customers the wrong order size.** In the founder's test it confidently told the customer it had "45 drinks" when it had really registered 35 — a direct path to wrong totals, wrong deliveries, and lost trust. The assistant now always works from the true, system-counted number.
- **Customers were getting spammed and confused.** The assistant fired off two or three separate WhatsApp messages within a second and sometimes affirmed something in one and walked it back in the next, reading as if the bot didn't know what it was doing. It now sends one clean message per turn.
- **A completed order looked "lost" to the customer.** When a human advisor didn't answer in time, the assistant reset the conversation and told the customer to begin again from scratch — even though the order was in fact saved for staff follow-up. Now the customer is told their order is safe and that an advisor will reach out.
- **Old buttons leaked into a conversational experience.** A reminder sent after a customer went quiet was still using the old tappable-button style, which is out of place in an assistant designed to work entirely through natural chat. That path now sends plain text, and a full audit confirmed no other button leaks exist in the live AI mode.
- **Stock reality wasn't enforced.** Alcohol-free drinks are currently out of stock for single-unit retail and only viable wholesale, but the assistant would still take small retail alcohol-free orders it couldn't fulfill. That rule is now enforced automatically.
- **Scheduling was unreliable and had no minimum notice.** The assistant didn't truly understand requested dates/times and had no floor on how soon a scheduled order could be. It now interprets the date the customer means, validates it, and requires at least 24 hours' notice so the team can actually prepare it.
- **Advisors couldn't see what they needed to pay ambassadors.** When a referral code was used, the advisor's order summary omitted the code, the customer discount, and the ambassador's commission. All three are now shown whenever a code applies.

## New Capabilities

- A more capable reasoning engine behind the assistant, chosen specifically to reduce arithmetic, date, and product-identification mistakes.
- One-message-per-turn replies, ending the rapid-fire multi-message behavior.
- An always-accurate, system-verified order count surfaced to the assistant so it can't overstate quantities.
- Automatic enforcement of the current alcohol-free "wholesale only" availability, built as a simple on/off switch for when retail stock returns.
- A new tamarind (with-alcohol) flavor available in the menu.
- Advisor order summaries that include the referral code, the customer's discount, and the ambassador's commission whenever a code is used.
- Genuine date/time understanding for scheduled orders with a firm 24-hour minimum lead time.
- A more forgiving advisor wait window with reassuring "your order is saved" messaging when it lapses.

## Business Benefits

- **Fewer costly order errors.** Correct counts and correct totals mean fewer disputes, fewer wrong deliveries, and less product given away or lost.
- **A more professional, trustworthy chat experience.** One coherent message per turn, no self-contradiction, and no stray buttons make the assistant feel like a competent human rep.
- **Orders stop feeling "lost."** Customers whose orders hit a delay now know the order is safe and a person will follow up — protecting real revenue that used to evaporate at that exact step.
- **Operations match reality.** The assistant only sells what can actually be fulfilled, and only schedules orders the team has time to prepare.
- **Ambassadors can be paid correctly.** Advisors now see the exact commission owed on every referral order, which is essential to running the ambassador program.

## Before vs After

- **Order count:** Before, the assistant could state a total that didn't match what it registered. After, it always uses the true count.
- **Message flow:** Before, bursts of two or three messages, sometimes contradictory. After, one clean message per turn.
- **Advisor didn't answer in time:** Before, the order was reset and the customer told to start over. After, the customer is told the order is saved and an advisor will reach out; the wait window is also longer.
- **Alcohol-free orders:** Before, small retail alcohol-free orders were accepted despite being unfulfillable. After, alcohol-free is wholesale-only, enforced automatically.
- **Scheduled orders:** Before, dates were stored as loose text with no minimum notice. After, the assistant understands the real date/time and requires at least 24 hours' notice.
- **Referral codes:** Before, advisors saw none of the code/discount/commission detail. After, all three appear on the order summary when a code is used.
- **Buttons in AI mode:** Before, a stray tappable-button reminder could appear. After, verified button-free for customers in live AI mode.

## Decisions

- **Upgrade the reasoning engine rather than only patching around its mistakes.** Several distinct failures (counting, dates, product confusion) all pointed to the assistant's underlying model being underpowered for the job. Upgrading addresses the common root while the added safeguards catch the rest. Cost stays bounded because there is already a daily per-customer usage cap.
- **Have the assistant resolve the requested date itself, then validate deterministically.** Rather than only instructing the assistant to "require 24 hours," the assistant now translates what the customer said into a concrete date and time, which the system then checks against a firm 24-hour rule — combining natural understanding with a reliable, non-negotiable guardrail.
- **Enforce the alcohol-free rule as a simple switch.** The wholesale-only limitation is a temporary stock reality, so it was built to be flipped back to normal retail in one place the moment stock returns.
- **Keep the older rule-based assistant and its button machinery intact.** The tappable-button code still exists because it powers the instant fallback to the previous rule-based ordering flow — the team's safety net. It was deliberately preserved rather than removed.
- **Make the advisor wait window more forgiving instead of eliminating it.** Ten minutes gives a real human a fair chance to respond during live operations, while the reassuring "order saved" message removes the downside of a lapse.

## Rejected Alternatives

- **Keep the existing model and rely only on stricter guardrails.** Rejected as the primary fix: guardrails can catch specific known mistakes, but the pattern of errors indicated a capability ceiling that a better engine addresses more broadly.
- **Enforce the 24-hour rule with only a written instruction to the assistant.** Rejected because it would depend on the assistant calculating dates correctly — the very thing that was failing. A deterministic check was chosen instead.
- **Remove the tappable-button machinery entirely.** Rejected because it is the mechanism behind the instant rollback to the older rule-based assistant; removing it would delete the team's safety net.

## Value Generated

This session turned a single real test conversation into a broad quality upgrade across the parts of the assistant that most directly affect money and trust: accurate order counts and totals, a professional one-message-per-turn chat feel, orders that no longer feel lost when a human is briefly unavailable, availability and scheduling rules that match what the business can actually deliver, and the referral-commission visibility needed to run the ambassador program. All of it was verified against the full automated test suite before being published.

## Features Added

- More capable reasoning engine powering the assistant.
- Single consolidated reply per conversation turn.
- Verified, always-correct order-count shown to the assistant.
- Wholesale-only enforcement for alcohol-free drinks, with a simple re-enable switch.
- New tamarind (with-alcohol) flavor in the menu.
- Referral code, customer discount, and ambassador commission on advisor order summaries.
- True date/time understanding plus a 24-hour minimum for scheduled orders.
- Longer, reassuring advisor-wait behavior that tells customers their order is saved.
- Full audit confirming no customer-facing buttons in live AI mode.

## Future Opportunities

- **Interactive follow-up when a human is slow on an immediate order.** Today, if an advisor doesn't respond in time, the customer is reassured the order is saved. A natural next step is to actively ask that customer whether they'd rather schedule the order for later or keep waiting while the assistant retries reaching a human — giving them control instead of just a status update.
- **Surface the wholesale-only alcohol-free rule proactively in the menu.** The rule is now enforced at checkout; presenting it up front would set expectations earlier and reduce back-and-forth.
- **Live testing on the real number.** Because there is no longer a private practice mode, validating the new behavior with a few real end-to-end test conversations on the live line is the recommended way to confirm the improvements in practice.
