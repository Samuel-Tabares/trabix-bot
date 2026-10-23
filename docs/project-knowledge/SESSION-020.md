# SESSION-020

## Executive Summary

This session started with an incident: a customer received a WhatsApp message from the ordering assistant at 8:00 in the morning on 20 October, without having written anything. We traced it from start to finish using the conversation records and the assistant's activity history.

The customer had sent a photo the afternoon before. The assistant passed the conversation to a person, as designed. The founder then sold and delivered the order by hand, but never handed the conversation back. Six hours after the founder's last message, at 23:05, the assistant took the case back by itself. It tried to read the human conversation and finish the order, and because the shop was closed it left the order "waiting for opening time". At 8:00 it told the customer their order was confirmed and asked how they wanted to pay, for an order they had already received. As side effects, the team's to-do queue showed an order that looked meaningless, two of the assistant's notes to the team were hidden, and the customer's contact was saved under the name ".".

We fixed this in two steps. A first release patched each failure. Then, following the founder's direction, a second release removed the root cause: when a conversation is handed back, the assistant now starts fresh with that customer. It no longer tries to continue what a person handled, never writes to the customer when it takes the case back, and leaves any cart it had prepared in the to-do queue for the team to accept or cancel. The founder can now also correct a customer's name, phone and address by hand from the internal console. The affected customer's data was corrected, both services were deployed, and the code is published.

## Objectives Achieved

1. ✅ Reconstructed exactly why a customer got an unsolicited 8:00 message, using real conversation and activity records
2. ✅ Removed the behavior where the assistant tried to finish an order a person had already handled
3. ✅ The assistant no longer writes to a customer when a conversation is handed back to it, at any hour
4. ✅ Orders the assistant had prepared before a hand-off are kept in the to-do queue, so the sale can be registered with one tap
5. ✅ The 8:00 "we're open, your order is confirmed" message now also updates the order itself (it used to say "confirmed" while the order stayed incomplete)
6. ✅ The team now always sees the assistant's notes; amounts the assistant can't verify are flagged instead of the note being hidden
7. ✅ Carts abandoned for two days or more no longer come back when the customer returns (one from July would have asked a customer how they'd pay for a large old order)
8. ✅ Customer contacts can be edited by hand from the internal console, and the corrected data is what the assistant uses on the next order
9. ✅ Corrected the affected customer's data and removed the misleading item from the to-do queue
10. ✅ Deployed both services to production and published the code

## Business Problems Solved

- **A customer was contacted out of the blue about an order they already had.** The assistant confirmed an already-delivered order and asked for payment. That looks careless to the customer and could have caused a double charge.
- **The to-do queue showed noise.** A duplicate order created by the assistant appeared there under the name "." and flagged as needing attention, which confused the founder.
- **The team could be left uninformed.** Notes from the assistant that mentioned amounts were replaced with "message blocked", so the team lost exactly the information it needed to check the case.
- **Customer records could not be fixed.** When a customer gave their name to a person during a hand-off, nobody recorded it, and the console had no way to correct it.
- **Old, unfinished orders could resurface.** A customer returning weeks later could be pushed back into an unfinished order flow.

## New Capabilities

- Handing a conversation back to the assistant, by button or automatically after six hours, is now a clean restart: no message to the customer, any prepared cart goes to the to-do queue, and the assistant is told the earlier conversation is closed.
- An "Edit contact" option in the internal console to correct name, contact phone and address.
- Notes to the team always arrive complete; amounts the assistant could not verify are marked for review.
- Automatic discard of carts left untouched for two days or more when the customer writes again, except cases still waiting on a person (a quote, a payment proof already sent, or an open hand-off).
- The banner shown while a person is handling a chat now explains that the assistant will start from scratch when the chat returns to it.

## Business Benefits

- Customers stop receiving confusing or badly timed messages, which protects the brand's trust.
- One place to close every hand-off order: the to-do queue, where accepting it records the sale and updates inventory.
- Customer records the founder can trust and fix, which feeds repeat-purchase follow-up.
- Lower AI cost per hand-off: taking a case back no longer uses the AI at all.
- A simpler system: less behavior that only works if a person remembers to press a button.

## Before vs After

| Aspect | Before | After |
|---|---|---|
| Conversation handed back after a person handled it | The assistant read the human chat, tried to finish the order and could message the customer at any hour | The assistant starts fresh, sends nothing, and leaves the cart in the to-do queue |
| Forgetting to press "hand back" | After 6 hours the assistant took over at whatever time it was, even 23:05 | After 6 hours the case returns silently; nothing reaches the customer |
| The 8:00 "we're open" message | Told the customer "confirmed" but left the order incomplete | Updates the order and notifies the team as intended |
| Assistant notes with amounts | Hidden as "message blocked" | Shown in full, with unverified amounts flagged |
| Customer name given to a person | Lost; the contact showed the WhatsApp profile name (".") | The founder edits it in the console and the assistant uses it next time |
| Customer returning weeks later | Could be dropped back into an old unfinished order | Starts a fresh order |

## Decisions

- **The assistant starts from zero after any hand-off** (founder's call). Everything during a hand-off is handled by a person, so the assistant should not try to continue it.
- **Keep both ways of handing back:** the button for an immediate return, and the 6-hour automatic return as a safety net. Since handing back no longer contacts the customer, the automatic return is safe at any hour.
- **The to-do queue is the single closing point** for orders that went through a person. Accepting there records the sale.
- **Contact edits go through the assistant's own system,** so the console never writes the assistant's data directly and the corrected name is also used on the next order.
- **Two days of inactivity defines an abandoned cart.**
- **Accepted trade-off:** orders closed after a hand-off no longer send the purchase signal to Meta's ads platform and no longer add to the customer's purchase history automatically.

## Rejected Alternatives

- **Keeping the "read the human chat and finish the order" behavior, only delaying it until opening time.** This was the first patch. It was replaced because it still relied on the AI reading amounts written by hand in a chat and still wrote to customers.
- **Only a hand-back button, with no automatic return.** Rejected: a forgotten button would leave the customer without the assistant indefinitely.
- **Letting the console edit customer data directly in the assistant's database.** Rejected to keep a single writer for the assistant's data.

## Value Generated

- Removed a class of incident that can damage customer trust and cause wrong charges.
- Cleaned the team's to-do queue and contact list.
- Lowered the AI cost of every hand-off to zero.
- Faster, more reliable sale registration for orders handled by a person.

## Features Added

- Clean restart of the assistant when a conversation is handed back
- Prepared carts preserved as orders in the to-do queue
- Contact editing in the internal console
- Full team notes, with unverified amounts flagged
- Automatic discard of abandoned carts
- The 8:00 reopening message now updates the order correctly
- Clearer banner while a person is handling a chat

## Future Opportunities

- Send the purchase signal to Meta's ads platform and update the customer's purchase history when the team accepts an order in the to-do queue. This would recover what is now lost for orders that went through a hand-off.
- Watch the first real hand-back that includes a cart and confirm the order appears in the to-do queue with no message to the customer.
- Revisit the two-day abandoned-cart threshold once there is real data on how long customers take to come back.
