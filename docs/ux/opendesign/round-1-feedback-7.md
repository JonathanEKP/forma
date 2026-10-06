A tester went through both layouts. Fix these. Each one breaks the user's trust or leaves them stuck.

1. **Success that isn't true.** Calm, scenario 03 (8 layers across two pages). After Approve and the device confirmation, the chat says "Deleted 8 layers… now v7". The Layers list still shows "Kicker" on Cover, and the validation chip flips from "Valid" to "2 issues" with no explanation. The approval's affected set has to be exactly what disappears: every listed layer is gone afterwards, on the right page. Validation after an approved deletion stays "Valid" unless the deletion really caused an issue; if it did, the agent's confirmation line names it. Check all 5 scenarios for this.
2. **"Show on canvas" must not open editing.** Today it selects the affected layers, so Properties opens with editable fields ("Headline · 4 selected · Editing the first selected layer") while the approval is pending. Make it a highlight of the affected layers (on the artboard and in the Layers list), not an editable selection. Properties stays closed and nothing affected is editable until the approval settles. The button keeps one meaning: "Show on canvas" toggles the highlight with a pressed state. It doesn't turn into "Clear selection".
3. **After deny, expire or cancel, say how to go on.** The one-line record stays, plus a quiet hint: "Ask the agent again if you change your mind." The repeat-after-deny line gets the same hint, so a mistaken Deny isn't a dead end.
4. **Controls that lie about being available:**
   - "Stop" is enabled only while the agent is actually working.
   - While Sending, Deny's disabled state says why ("Sending your answer…").
   - A blocked Send says why and what unblocks it ("The agent is working. Stop it to send now.").
5. **Calm, things that move under the user:**
   - The icon rail gets tooltips with the panel names, and the open one shows its name in the flyout header.
   - The zoom percentage is stable for the same artboard and window size. It must not read 12% in one state and 34% in another.
   - The zoom control and the "Select a layer to inspect it" hint keep their positions after an approval.
   - The "Undo restores…" note becomes part of the agent's confirmation line in the chat ("Deleted the layer “Headline”. Undo brings it back."). Today it is a caption under the artboard that vanishes on the next selection.
