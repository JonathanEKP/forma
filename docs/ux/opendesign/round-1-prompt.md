Prototype the **approval card** of Forma's in-editor Agent chat, as an interactive single-page prototype that I can review state by state.

## Context

Forma is a local design editor where an AI agent edits the person's design through a chat panel on the right of the editor. When the agent wants to do something destructive (remove layers or pages), Forma asks the person first with an approval card inside the chat. The person answers "Approve once" or "Deny"; there is no session-wide grant. If nobody answers, the approval expires after 2 minutes.

Today's card (see the attached screenshot C05 if you have it) is bad:
- It says "Remove one element from this design, as one new version", then raw parameters: `design_id 4e92dd26-f076-…`, `base_version 6`, `operations 1 operation: remove_page`, "Affects 1 item." Removing the Footer and removing the Headline produce identical cards.
- Removing a page that holds 3 layers is described as "1 item".
- With the Agent panel closed, the only signal is a tiny "1" badge on the Agent button, and the card never shows its deadline.

## Frame

Render the whole editor as context so the card is judged where it lives, using the selected Forma Editor design system (dark Ink chrome, warm neutrals, Geist, Signal orange #FF5A36 only for what is active):
- Top bar: "← Projects", design name "ALPHA Launch cover" + "v6", zoom "100%", Export, Undo, then toggles Agent (active), Properties, Validation, Assets, Brand.
- Left column: Pages ("01 Cover", "02 Back") and Layers of the current page (Footer · Text, Tagline · Text, Headline · Text, Accent block · Rectangle).
- Center: canvas well with a Paper-colored portrait artboard showing a big "ALPHA LAUNCH" headline, a tagline and a footer (simple, it is context).
- Right: the Agent panel (~360px): a compact status header ("● Connected · live", device "My laptop · online"), the transcript (a user message, an agent reply, one or two compact "Running in Forma" tool rows), the approval card, and the composer at the bottom (textarea "Message the agent", Send, Stop).

## The new card

Content, in the person's words:
- Heading row: "NEEDS YOUR APPROVAL" label, a "DESTRUCTIVE" risk badge, and on the right a countdown "Expires in 1:42" (ticking).
- The action as one sentence with real names, e.g. **Delete the layer “Headline” from “ALPHA Launch cover”**.
- What is destroyed, counted by what disappears (pages + the layers on them + loose layers), never by number of operations. Up to 5 names, then "+N more".
- A small "Show on canvas" text button (only meaningful when the design is open in this window): clicking it highlights/selects the affected layers in the layers list and on the artboard.
- A collapsed "Details" disclosure holding the technical values (design id, base version, raw operations) in Geist Mono, faint.
- Buttons: "Approve once" (primary, Signal) and "Deny" (secondary).

## Make these switchable (a small floating control or a sidebar outside the editor frame, clearly not part of the product)

Cases:
1. One layer: Delete the layer “Headline” from “ALPHA Launch cover”.
2. A page with three layers: Delete page 2 (“Back”) of “ALPHA Launch cover” and the 3 layers on it: Footer, Tagline, Headline.
3. Eight layers across two pages (shows "+3 more").
4. A batch that removes one layer and also edits two others (say the edits plainly but quieter than the removal).
5. A design the person cannot read: no names are available; the card must still be honest ("Delete 1 layer from a design you can't open here").

States:
- Pending (countdown ticking; under 30s the countdown turns more urgent but not alarming).
- Sending… (buttons disabled, after a click).
- Not confirmed by your device (after 20s without confirmation: buttons come back, a quiet explanatory line).
- Approved, Denied, Expired before you answered, Cancelled — the settled card collapses to a compact one-line record that still names what it was about.
- Panel closed: show the top bar with the Agent toggle carrying an attention signal, and a toast or the panel auto-opening; also show the browser tab title as "(1) Approval needed · Forma" in a fake tab strip above the frame.
- Repeat after deny: a quiet chat line "You declined this earlier in this session, so it wasn't asked again."

## Constraints

- Signal orange only for the primary action, the countdown when urgent, focus and selection. No walls of orange, no gradients, no glows, no sparkles, no purple/blue.
- Dense, professional tool; 12–13px text in the panel; 8px radius; hairline warm borders.
- Plain English, sentence case. Never show internal ids outside "Details".
- Accessible: every button has a specific accessible name (e.g. "Approve deleting the layer Headline"), visible focus rings, sufficient contrast on Ink.
