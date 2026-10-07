# UX batch 1 — owner review

**Run:** 2026-10-06, browser-stack instance 30, real Codex (`gpt-6-luna`, effort low), person A, on `main` at `40b40f4`. The owner used the editor and chat by hand after batch 1 was merged.

## Findings

| Id | Where | What the owner saw | Decision | Status |
|---|---|---|---|---|
| UX1-4 | Editor, Properties panel | The panel has no close button. Its only exit is deselecting, which nobody guesses. | **Owner decision (2026-10-06): as the editor prototype (`index.html` Calm).** Properties stays the column to the right of the canvas, with a × in its header; Escape and deselection also close it. Closing does not change the selection, and selecting a layer reopens it. | Superseded by UX1-8 |
| UX1-5 | Editor, Export | The export options open, but there is no obvious way to close them. | **Owner decision (2026-10-06): as the editor prototype (`index.html` Calm).** A popover under Export with a header ("Export" and ×), the design name and "1080 × 1440 · portrait", then the build's real page and scale options. Escape (focus returns to Export), × and a click outside close it. No backdrop, no modal (C-5). An export already running keeps running. | Superseded by UX1-8 |
| UX1-6 | Chat, transcript scrollbar | The scrollbar style is not liked. | Invisible until used: no visible bar at rest; a thin, semi-transparent overlay thumb appears while the pointer is over the transcript or it is scrolling, then fades. The content width does not shift when it appears. Keyboard and wheel scrolling are unchanged. | fixed (e513cae) |
| UX1-7 | Chat, Jump to latest | The text button sits in the layout between the transcript and the composer, cutting the conversation, and looks plain. | A small round floating button with a ↓ arrow, overlaid on the bottom-right of the transcript. It takes no layout space and has the accessible name "Jump to latest". When a new entry arrives while the person is scrolled up, it shows a small Signal dot. It appears and disappears with a short fade, and the follow rule of UX1-3 is unchanged. | fixed (8f636f0) |

UX1-6 and UX1-7 were browser-proved on FakeProvider instance 32, then stopped. The [transcript at rest](evidence/ux1/G-6-scrollbar-rest.png) and [on hover](evidence/ux1/G-6-scrollbar-hover.png) kept the same content width; the [Jump to latest state](evidence/ux1/G-7-jump-latest.png) shows its Signal dot while scrolled up.

## Fidelity to the approved prototypes

The owner noticed the build is plainer than the OpenDesign prototypes they approved. A side-by-side comparison on instance 30 (prototype `index.html` Calm and `chat.html` against the live editor, at 1595×937) found the gaps below. The owner chose to align all of them with the prototypes, and asked for a full screen-by-screen audit of all four prototypes before fixing, recorded in [ux-batch-1-fidelity.md](ux-batch-1-fidelity.md).

| Id | Where | Prototype | Build |
|---|---|---|---|
| UX1-8 | Editor, side panels | One left flyout, about 240px wide, with a title and ×, holds whichever tool is open: Layers & pages, Assets, Brand, Properties (opens on selection) and Export (file name plus Download). The canvas keeps the rest of the width. | Properties is a separate column between the canvas and the chat, squeezing the canvas. Export is a dropdown under its button. |
| UX1-9 | Editor, canvas | A mono label above the artboard ("01 Cover / 1080 × 1440"), and a status line at the bottom left ("Headline selected · page 1", or "Select a layer to inspect it."). Zoom is "– Fit +". | Neither the label nor the status line exists. |
| UX1-10 | Editor, layer list | A type icon per row (T text, □ shape, image) and counts in the section headings ("Pages 2", "Layers 4"). | A grey dot per row, the type written on the right, and uppercase headings without counts. |
| UX1-11 | Editor, chat entry | A chat icon at the bottom of the left icon rail when the chat is closed. | A floating "Ask the agent…" bar under the canvas. |
| UX1-12 | Chat, header | "Agent" with an ⓘ icon button; one line "• Connected" … "My laptop"; a mono sub-line "This window's session". Provider, sandbox and sign-in detail live in the ⓘ popover. | Text buttons "Info · New session · Close", and a permanent block with the provider, its sign-in state and sandbox chips. |
| UX1-13 | Chat, composer | One box: label "Message the agent", placeholder "Ask for a change…", and a footer inside it with the state ("Ready") on the left and "Send ↑" on the right. Below it: "Only this window's view is shared." | A bare textarea, then a dark red Send block outside it, then "Write a message to send." |
| UX1-14 | Chat, context chip | An icon, then a sentence: “ALPHA Launch cover” · page 1 · “Headline” selected. | A boxed "CURRENT VIEW" label, then the names without quotes or the word "selected". |

### Owner decisions on the audit (2026-10-06)

The audit ([ux-batch-1-fidelity.md](ux-batch-1-fidelity.md)) found the two prototypes disagree on UX1-8: `index.html` keeps Properties as a right-hand column and Export as a popover, `chat.html` opens Export in the left flyout. The owner chose **the editor prototype**: UX1-8 is closed by UX1-4 and UX1-5 as decided above, and no shared left flyout is built for Properties or Export.

- **Wording of stopped notices (UX1-37…UX1-43): keep "computer".** Round 3 named the thing "computer", so the build's notices stay and the `chat.html` "device/Runtime" wording is not copied. The one exception is the offline-during-a-turn notice (UX1-38, UX1-66), which takes the round-3 `devices.html` wording: "{name} went offline during this turn." / "Start it on your computer, then begin a new session. To use another online computer, choose it below first." (the last sentence only when another computer is online).
- **UX1-11 stays as built.** The round-1 collapsed composer ("Ask the agent…") replaces the prototype's rail chat icon by decision.
- **To fix, as the prototypes show:** UX1-9, UX1-10, UX1-12, UX1-13, UX1-14, the approval helper "Only this request. No permission for future changes." (UX1-15, UX1-53), the "Your session is ready." / "Send a message when you are ready." info notice (UX1-46), and the MCP page layout (UX1-69).

## Question answered, no change

**"Opening another project showed the same agent history. Is that normal?"** Yes, by design: the chat belongs to the window, not to a project or design (`App.tsx`; CLAUDE.md invariants). Each message carries the `[forma-view]` line for where the person is when they send it, so the agent acts on the current design. A new conversation starts only with **New session** or a Run on change (U-2). A per-project history was never decided and is not in scope.
