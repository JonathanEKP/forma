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

## Decisions

*(filled in as the round is reviewed)*
