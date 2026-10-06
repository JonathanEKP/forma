# UX round 1: the approval card

**2026-10-04.** First round of the UX work agreed after fix batch 2 (see the Progress table in
[pontis-integration-plan.md](../pontis-integration-plan.md) §11). Prototyped in OpenDesign, a local
tool, against the [Forma editor design system](opendesign/forma-editor-DESIGN.md); **decisions are
recorded here, not there** — OpenDesign is the sketchpad, this file is the record a Codex worker
implements from.

## The problem (browser findings)

- **F-30-1** — the card never says which layer or which design: removing the Footer and removing the
  Headline produce identical cards (`design_id` UUID, `operations: 1 operation: remove_layer`).
- **F-30-2** — a page removal reads as "one element"; the three layers on it are not mentioned.
- **F-30-7** — a pending approval is easy to miss (a small "1" on the Agent button when the panel is
  closed), the card hides its 2-minute deadline, and after a deny Pontis silently refuses the same
  action again in that session without the chat saying it remembered.

Current component: `apps/web/src/pontis/ChatApproval.tsx`. Current sentence: composed in
`apps/pontis-sidecar/src/capabilities.ts` (`requiresApproval` of `design.applyOperations`).

## Starting recommendations (to be validated visually)

1. **The sidecar writes the sentence, with real names.** Pontis's `requiresApproval` hook is async
   and receives the person, so the sidecar reads the design through Boundary F *as that person* (the
   rule the `[forma-view]` line already follows) and writes the summary and reason with layer names,
   the design name and page numbers. A design the person cannot read gets no names. Not the editor:
   it only knows names for a design open in that window, and the sidecar's sentence is the one Pontis
   stores and shows on any surface.
2. **What the card says** — the action in the person's words; the count is what is destroyed (pages,
   plus the layers they hold, plus loose layers), not the number of operations; up to five names then
   "+N more"; ids, version and raw operations behind a collapsed "Details".
3. **"Show on canvas"** — only when that design is open in this window; selects the affected layers
   through `useEditor`, exactly as a click would (no private path).
4. **Hard to miss** — a visible countdown from `expiresAt`; if the panel is closed in the window that
   owns the chat, the panel opens and the tab title becomes "(1) Approval needed · Forma"; no sound.
   When Pontis refuses a repeat of a denied action, the chat says "You declined this earlier in this
   session".
5. **Kept** — the risk badge, "Approve once" / "Deny" as the only answers (no session-wide grant in
   PV-1), and the "Sending…" / "Not confirmed by your device" states from fix batch 2.

## States and cases to prototype

States: pending (with countdown), sending, not confirmed by your device, approved, denied, expired,
cancelled; the panel-closed signal.

Cases: one layer ("Delete the layer “Headline” from “ALPHA Launch cover”"); a page with three layers;
a batch of eight layers across two pages; a removal mixed with ordinary edits in one batch; a design
the person cannot read (no names available); the repeat-after-deny line.

## Learned while prototyping

- **"A design the person cannot read" cannot happen.** The agent acts as the person through Boundary F, so it
  can never remove anything from a design they cannot read. The real no-names case is that the sidecar's read
  for names failed or timed out when it wrote the card: "Delete 1 layer from “ALPHA Launch cover”" (or "from this
  design"), then "Forma couldn't load the layer's name. Details shows its id." Recommendation 1 changes
  accordingly: names are best-effort, the ids are always the authority.
- When a batch spans pages, each chip carries its page as a small suffix ("Headline · Cover"); a page chip
  carries a page icon.
- Settled cards collapse to one line that says the outcome in the person's words: "Approved. Deleted the layer
  “Headline”." / "Denied. “Headline” was kept." / "Expired before you answered. Page “Back” and its 3 layers were
  kept." The tool row above it reads "Not run" when nothing executed.
- While "Sending…", the countdown is hidden; an approval that expires while sending still settles as expired.
- After an approved deletion the editor shows the new version (v6 → v7) and the agent confirms in one line;
  Undo goes to v8, never back to v6.
- **Open: the countdown after the person has answered.** A tester saw the deadline keep running while an
  approval waited for the device's confirmation, so a slow confirmation could expire an answer already given.
  It has to be decided against how Pontis actually times out `approval.resolve` (item 11 of the upstream
  report), not in the prototype.

## A calmer editor ("Calm" layout, proposed)

Asked for by the owner after the card was approved: the editor read like a CRM (eight text buttons in the
top bar, a boxed header on every panel). Prototyped as a second layout, switchable in the review rail:

- The canvas is the room; regions separate by tone, not boxed borders.
- Top bar: Forma symbol, "Projects /" breadcrumb, design name, quiet version tag; on the right Undo/Redo
  icons, a validation chip ("✓ Valid" / "2 issues", opens a popover) and Export as the one primary action.
- Left: a slim icon rail (Layers & pages, Assets, Brand), one flyout at a time; pages as a switcher on top
  of the layers.
- Properties appears only while something is selected.
- Zoom is a floating control at the bottom-right of the visible canvas, showing the real percentage.
- The agent is a presence: collapsed, a floating composer at the bottom-centre of the canvas ("Ask the
  agent…"); expanded, the chat docks on the right. An approval while collapsed shows a labelled bubble
  ("Needs your approval", sentence, countdown, Review) above the composer.

This stays inside plan §10's regions (pages and layers left, canvas centre, properties right, Export top
right); it changes how they are shown, not where they are.

## Decisions

**2026-10-05, approved by the owner** after seven feedback passes and two test rounds (one by a Sonnet tester).
The prototype is in OpenDesign; the record is [opendesign/](opendesign/).

### The approval card

1. **The sidecar writes the sentence, best effort, with ids as the authority.** It writes it in `requiresApproval`, reading
   through Boundary F as the person: "Delete the layer “Headline” from “ALPHA Launch cover”", "Delete page 2 (“Back”)
   of … and the 3 layers on it", "Delete 8 layers across 2 pages of …". If the read for names fails or times out, it
   writes "Delete 1 layer from “…”" (or "from this design") followed by "Forma couldn't load the layer's name. Details
   shows its id." It never blocks on the read.
2. **Content.** The card's parts, in order:
   - a heading row with "NEEDS YOUR APPROVAL", the DESTRUCTIVE badge and a right-aligned "Expires in m:ss";
   - the sentence;
   - the count of what disappears, with pages and layers counted separately ("8 layers will be deleted. Both pages
     stay.");
   - up to five chips and then "+N more"; a chip carries a page suffix only when the batch spans pages, and a page
     chip carries a page icon;
   - "Show on canvas";
   - a collapsed "Details" with the ids, the base version and the raw operations in Geist Mono;
   - "Approve once" (primary) and "Deny".

   In a mixed batch the removal comes first and the ordinary edits follow, quieter.
3. **"Show on canvas" highlights; it does not select.** It shows a Signal outline on the artboard and in the Layers
   list, and it is a toggle with a pressed state. Properties stays closed, and while the approval is pending the
   affected layers are read-only. It appears only when the design is open in this window. This replaces
   recommendation 3's "selects through `useEditor`": a selection would open editing on what is about to be deleted.
4. **Hard to miss.**
   - The countdown comes from `expiresAt` and turns urgent, not alarming, under 30 seconds.
   - It is hidden while sending.
   - When the panel is closed, the Calm bubble appears (see below) and the tab title becomes "(1) Approval needed ·
     Forma". There is no sound.
5. **States.**
   - **Sending:** "Sending your answer… Waiting for confirmation from {device}." The buttons are replaced, not merely
     greyed.
   - **Not confirmed by your device:** after 20 seconds the buttons come back with a quiet explanation.
   - **Settled:** one line in the person's words, the tool row above it reads "Not run" when nothing executed, and a
     denied, expired or cancelled line adds "Ask the agent again if you change your mind.":
     - Approved: "Approved. Deleted the layer “Headline”."
     - Denied: "Denied. “Headline” was kept."
     - Expired: "Expired before you answered. … was kept."
     - Cancelled: the same form.
   - **Repeat after deny:** "You declined this earlier in this session, so it wasn't asked again." plus the same hint.
6. **After an approved deletion** the version goes up (v6 → v7). The agent confirms in one line that names what was
   removed, says what was deliberately kept, ends with "Undo brings it back.", and names any validation issue the
   deletion caused. Undo goes forward (v8).
7. **Controls say why they are unavailable:**
   - Stop: "No agent task is running".
   - Send: "Write a message to send" or "The agent is working. Stop it to send now."
8. **Open: the countdown after the person has answered** (see *Learned while prototyping*). This is decided
   against Pontis's resolve timeout, not here.

### The editor: adopt Calm

The "Calm" layout above is adopted for the editor, in place of the toggle row. Two more points apply:

- The icon rail has tooltips.
- Zoom is stable for the same artboard and window and does not move after an approval.

It stays within plan §10's regions, so it is a presentation change, not a plan change. It is a separate implementation
task from the card, and the card does not depend on it: in the current layout the bubble is the toast at the Agent
panel's edge.
