# SESSION-016

## Executive Summary

Real customers testing the AI ordering assistant during the trial period surfaced a set of concrete problems, all captured in a single written list at the end of the last session. This session worked through that list one item at a time, with the founder confirming the plan before each fix. For every issue, the underlying calculation logic (pricing, delivery fees, discounts) was independently verified against real examples first — and in every case it was already correct. The actual defects were all in how the assistant communicated and sequenced things with customers, not in the math itself. Four of the highest-priority problems were fixed and safety-tested: the assistant quoting made-up prices, delivery fees being left out of quoted totals, scheduled orders getting stuck in the wrong approval flow, and the assistant mismatching drink flavors that exist in two different versions. The remaining items on the list are left for a future session.

## Objectives Achieved

1. ✅ Verified that the order-pricing calculations (regular price, bulk discounts, buy-one-get-one promotions, delivery fees) are correct in every real scenario tested — the reported pricing errors were confirmed to be a communication problem, not a math problem
2. ✅ Stopped the assistant from ever quoting a price to a customer that the system itself hadn't actually confirmed
3. ✅ Fixed quoted totals so they can no longer omit the delivery fee while still being labeled as the final amount owed
4. ✅ Fixed scheduled orders (as opposed to immediate ones) so they no longer get stuck waiting on an approval step that was never supposed to apply to them
5. ✅ Made the system more reliable at forwarding a customer's payment receipt photo to staff for verification
6. ✅ Fixed the assistant confidently guessing which version of a drink a customer wanted when the name alone doesn't say — it now asks instead of guessing
7. ✅ All fixes were verified against the automated test suite before moving to the next item, with zero regressions introduced

## Business Problems Solved

### 1. The Assistant Sometimes Quoted a Price It Made Up Instead of the Real One
**Problem:** In live testing, the assistant told a customer a total that didn't match what the actual order-pricing system had calculated (off by several thousand pesos on a real order). Investigation showed the underlying pricing engine was correct in every test case — the assistant was occasionally stating a number out of its own "memory" of the conversation instead of reading it from the system's actual calculation, and in at least one case did this for an item it hadn't even recorded as part of the order yet.

**Solution:** Added an automatic safety check that runs on every single message before it reaches the customer: if a message mentions a peso amount that doesn't match a real, system-confirmed figure from that conversation, the message is blocked and replaced with a safe placeholder, and the incident is logged for review. This does not rely on the assistant "remembering" to behave — it is enforced independently every time.

**Impact:** A customer can no longer be quoted a price that doesn't match what they will actually be charged. This closes the single highest-risk defect on the list, since it directly involves real money.

### 2. Delivery Fees Could Be Silently Missing From a "Final" Total
**Problem:** When a customer asked for their total before the delivery zone had been established, the assistant would show a number labeled as the final total that was actually only the product cost — the delivery fee hadn't been added yet, and there was no indication one was still coming.

**Solution:** The order summary now explicitly distinguishes "delivery fee not yet known" from "delivery is confirmed and costs a specific amount" — a total is only ever labeled "Total" once the delivery fee is actually included; until then, it's clearly marked as a products-only subtotal.

**Impact:** Customers can no longer be shown an incomplete total that looks final. This removes a second real-money risk found during testing.

### 3. Scheduled Orders Were Stuck Waiting on an Approval Step That Doesn't Apply to Them
**Problem:** By business rule, an order scheduled for a future date/time is meant to be accepted automatically — staff should never have to separately confirm they're available to fulfill it (that check only makes sense for "right now" orders). In testing, scheduled orders were incorrectly being routed through that same availability check anyway, which also disrupted the wire-transfer payment flow for scheduled orders (customers received confusing or incomplete instructions, and there were signs the payment receipt photo wasn't reliably reaching staff for verification).

**Solution:** Scheduled orders now follow their own dedicated path: as soon as the delivery cost is known, the order is automatically accepted, the total is calculated, and the customer is moved straight to choosing a payment method — staff receive an informational notice rather than a request for approval. The system now actively refuses to run the "confirm availability" step on a scheduled order at all, so this mistake can't recur even if something upstream tries to trigger it. Receipt-photo forwarding to staff was also made more resilient, so a photo can no longer fail to reach staff due to a timing mismatch in the conversation's internal tracking.

**Impact:** Scheduled orders paid by transfer now follow the intended, correct business flow end to end, and staff no longer receive availability questions for orders that were always meant to be auto-approved.

### 4. The Assistant Sometimes Guessed the Wrong Version of a Drink
**Problem:** Several drinks exist as genuinely different products depending on whether they include alcohol (for example, a plain fruit-flavored drink versus the same flavor made with a specific liquor) — these aren't variants of one product, they're separate menu items. When a customer named only the base flavor without saying which version they meant, the assistant would sometimes silently guess based on the flow of conversation, and occasionally guessed wrong.

**Solution:** The four flavors that exist in more than one version now require the customer's own wording to clearly indicate which one they meant (a liquor name, "with/without alcohol," etc.) before the assistant is allowed to add it to the order. If the customer's phrasing doesn't make it clear, the system rejects the attempt and requires the assistant to ask — it can no longer add a guessed version on its own. Flavors that only exist in one version are unaffected and continue to be added directly without extra questions.

**Impact:** Customers can no longer be sold — or accidentally charged for — the wrong version of a drink because of an unstated assumption.

## New Capabilities

- Automatic, real-time protection against the assistant quoting an unconfirmed price to any customer or staff member
- Order totals that clearly distinguish "still missing the delivery fee" from "this is the final amount"
- A dedicated, correct approval path for scheduled orders that no longer depends on the same steps used for immediate orders
- More reliable delivery of payment receipt photos to staff for scheduled, transfer-paid orders
- Mandatory clarification questions when a customer names a drink flavor that exists in more than one version, instead of a silent guess

## Business Benefits

- **Removes two direct financial-risk defects** (invented prices, incomplete "final" totals) that could have led to undercharging or customer disputes during the trial
- **Scheduled orders paid by transfer now work as designed**, removing a broken flow that could have stalled or confused real customers mid-order
- **Fewer wrong-item orders**, since the assistant can no longer silently pick the wrong (alcoholic vs. non-alcoholic) version of a flavor on the customer's behalf
- **Confirmed, not assumed, correctness**: every fix started by independently checking the actual calculation logic against real order examples, which showed the core pricing and discount math has been reliable throughout — narrowing every fix to the actual root cause instead of a broader rewrite

## Before vs After

**Before:** The AI assistant was live and mostly functional, but real customer testing had surfaced money-affecting communication defects: prices that didn't match the real calculation, totals that silently excluded the delivery fee, a broken flow for scheduled orders paid by wire transfer, and a chance of adding the wrong version of certain drinks.

**After:** All four of the highest-severity issues found in testing are fixed and independently verified with automated checks: prices shown to customers are now guaranteed to match the system's real calculation, totals are unambiguous about whether delivery is included, scheduled orders follow their correct dedicated approval path, and ambiguous drink orders now prompt a clarifying question instead of a guess.

## Decisions

1. **Work the fix list one item at a time, with explicit sign-off before each one.** Rather than batch-fixing everything found in testing, each item was investigated, confirmed, and only then fixed — keeping the founder in the loop on the diagnosis before any change was made.
2. **Verify the underlying calculations before touching any code.** For every item, real numbers from the actual test conversations were run through the pricing/discount logic first. In every case the math held up, which meant the fixes could be narrowly targeted at communication and sequencing issues instead of the calculation engine itself.
3. **Defer the remaining lower-priority items to a future session**, rather than rushing through the full list in one sitting. The founder chose to continue the rest later.
4. **Leave one piece of unused, dormant calculation code in place for now** (a since-superseded all-in-one order+delivery calculator) rather than removing it during this session — flagged for a future decision rather than deleted on the spot.

## Rejected Alternatives

- **Rewriting the pricing/discount engine "to be safe"** was considered unnecessary and rejected — direct testing against real order scenarios showed it was already correct; the fix effort went into the assistant's communication behavior instead.
- **Relying only on stronger wording in the assistant's instructions** to stop invented prices was rejected as insufficient on its own (this exact class of problem had recurred before through instruction-only fixes) — an automatic, independent check was added instead so correctness doesn't depend on the assistant "remembering" the rule.
- **Fixing all remaining list items in one long session** was rejected in favor of stopping after the highest-priority fixes and resuming later with fresh review.

## Value Generated

The trial period's real-money risk was substantially reduced: the two defects most likely to cost the business money or damage customer trust (invented prices, incomplete totals) are now closed off by automatic checks rather than by hoping the assistant behaves correctly. A structurally broken order path (scheduled orders paid by transfer) was corrected so that combination of choices — likely to become more common as the trial continues — now works end to end. And a subtler but real risk (silently selling the wrong version of a drink) was eliminated for every flavor where that confusion was possible.

## Features Added

- Automatic blocking of any customer- or staff-facing message that quotes an unconfirmed peso amount
- Clear "delivery fee pending" vs. "final total" labeling on every order summary
- A dedicated automatic-approval path for scheduled orders, separate from the immediate-order approval flow
- More resilient payment-receipt-photo forwarding to staff
- Mandatory clarification prompts for drink flavors that exist in more than one version

## Future Opportunities

- Continue working through the remaining items from the trial feedback list: enforcing business hours awareness more strictly, requiring a final customer confirmation recap before any order is submitted, removing interactive menu buttons in favor of fully natural conversation, fixing text formatting (bold text rendering incorrectly), requiring a discount/referral-code prompt on every bulk order, resolving a case where adjusting an already-confirmed order created a duplicate record instead of updating it, and separating a customer's official contact info (from WhatsApp) from any name/phone they choose to type in manually
- Decide whether to remove the unused all-in-one order-and-delivery calculator that was found dormant in the code, or wire it in for future use
- Once the remaining fix-list items are closed out, this would be a natural point to cut a version release documenting the full canary-period defect list and its resolution
