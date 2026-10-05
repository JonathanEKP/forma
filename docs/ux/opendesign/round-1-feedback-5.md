"Calm" is the right direction — much better. Tested it; fix these in Calm (Current can stay as is):

1. **Unstyled light scrollbar** on the canvas (a white vertical bar at its right edge). Style scrollbars thin and warm (Edge strong thumb, transparent track) everywhere, or hide the canvas one.
2. **Overlaps at the bottom of the canvas:** when Properties opens, the zoom control slides under the floating composer ("00% +" clipped). The artboard also sits under the approval bubble and the composer. Reserve a bottom safe area: fit-to-view leaves room for the floating composer, and zoom sits at the bottom-right of the *visible* canvas, never under the composer or the Properties panel.
3. **Approval bubble (agent collapsed):** add a small "Needs your approval" label with the destructive marker above the sentence, so it is not mistaken for a chat message; keep the countdown and "Review". Give it the same 8px radius and Edge strong border as the card.
4. **Floating composer icons are ambiguous** (a square, an arrow, a corners icon). Use: send arrow (disabled while empty), Stop only while the agent is working (with the label "Stop" on hover), and "Open chat" with a clear dock/panel icon. Every icon gets a tooltip.
5. **Per-layer properties:** selecting Tagline shows Footer's geometry (Y 0, height 64). Each layer has its own X/Y/W/H and fields.
6. The Validation popover covers the Properties header; anchor it under the chip and above everything, width ~280px, with the issue list (try one scenario with "2 issues" so the non-valid state is visible).
7. The chat icon at the bottom of the left rail duplicates the floating composer. Remove it, or make it the only way to open the dock and drop the corners icon — pick one entry point.
8. Re-test both layouts across every scenario and state after the changes.
