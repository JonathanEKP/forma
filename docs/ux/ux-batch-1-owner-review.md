# UX batch 1 — owner review

**Run:** 2026-10-06, browser-stack instance 30, real Codex (`gpt-6-luna`, effort low), person A, on `main` at `40b40f4`. The owner used the editor and chat by hand after batch 1 was merged.

## Findings

| Id | Where | What the owner saw | Decision | Status |
|---|---|---|---|---|
| UX1-4 | Editor, Properties panel | The panel has no close button. Its only exit is deselecting, which nobody guesses. | **Owner decision (2026-10-06): as the editor prototype (`index.html` Calm).** Properties stays the column to the right of the canvas, with a × in its header; Escape and deselection also close it. Closing does not change the selection, and selecting a layer reopens it. | fixed (3f6873d) |
| UX1-5 | Editor, Export | The export options open, but there is no obvious way to close them. | **Owner decision (2026-10-06): as the editor prototype (`index.html` Calm).** A popover under Export with a header ("Export" and ×), the design name and "1080 × 1440 · portrait", then the build's real page and scale options. Escape (focus returns to Export), × and a click outside close it. No backdrop, no modal (C-5). An export already running keeps running. | fixed (3f6873d) |
| UX1-6 | Chat, transcript scrollbar | The scrollbar style is not liked. | Invisible until used: no visible bar at rest; a thin, semi-transparent overlay thumb appears while the pointer is over the transcript or it is scrolling, then fades. The content width does not shift when it appears. Keyboard and wheel scrolling are unchanged. | fixed (e513cae) |
| UX1-7 | Chat, Jump to latest | The text button sits in the layout between the transcript and the composer, cutting the conversation, and looks plain. | A small round floating button with a ↓ arrow, overlaid on the bottom-right of the transcript. It takes no layout space and has the accessible name "Jump to latest". When a new entry arrives while the person is scrolled up, it shows a small Signal dot. It appears and disappears with a short fade, and the follow rule of UX1-3 is unchanged. | fixed (8f636f0) |

UX1-6 and UX1-7 were browser-proved on FakeProvider instance 32, then stopped. The [transcript at rest](evidence/ux1/G-6-scrollbar-rest.png) and [on hover](evidence/ux1/G-6-scrollbar-hover.png) kept the same content width; the [Jump to latest state](evidence/ux1/G-7-jump-latest.png) shows its Signal dot while scrolled up.

## Fidelity to the approved prototypes

The owner noticed the build is plainer than the OpenDesign prototypes they approved. A side-by-side comparison on instance 30 (prototype `index.html` Calm and `chat.html` against the live editor, at 1595×937) found the gaps below. The owner chose to align all of them with the prototypes, and asked for a full screen-by-screen audit of all four prototypes before fixing, recorded in [ux-batch-1-fidelity.md](ux-batch-1-fidelity.md).

| Id | Where | Prototype | Build |
|---|---|---|---|
| UX1-8 | Editor, side panels | One left flyout, about 240px wide, with a title and ×, holds whichever tool is open: Layers & pages, Assets, Brand, Properties (opens on selection) and Export (file name plus Download). The canvas keeps the rest of the width. | Closed by the owner decision: Properties remains a right-hand column and Export is an anchored popover, as specified under UX1-4 and UX1-5. |
| UX1-9 | Editor, canvas | A mono label above the artboard ("01 Cover / 1080 × 1440"), and a status line at the bottom left ("Headline selected · page 1", or "Select a layer to inspect it."). Zoom is "– Fit +". | Fixed (3f6873d): caption uses the real page index, name and format; the status reflects selection. |
| UX1-10 | Editor, layer list | A type icon per row (T text, □ shape, image) and counts in the section headings ("Pages 2", "Layers 4"). | Fixed (3f6873d): rows use type icons and the section headings include counts. |
| UX1-11 | Editor, chat entry | A chat icon at the bottom of the left icon rail when the chat is closed. | A floating "Ask the agent…" bar under the canvas. |
| UX1-12 | Chat, header | "Agent" with an ⓘ icon button; one line "• Connected" … "My laptop"; a mono sub-line "This window's session". Provider, sandbox and sign-in detail live in the ⓘ popover. | Text buttons "Info · New session · Close", and a permanent block with the provider, its sign-in state and sandbox chips. |
| UX1-13 | Chat, composer | One box: label "Message the agent", placeholder "Ask for a change…", and a footer inside it with the state ("Ready") on the left and "Send ↑" on the right. Below it: "Only this window's view is shared." | A bare textarea, then a dark red Send block outside it, then "Write a message to send." |
| UX1-14 | Chat, context chip | An icon, then a sentence: “ALPHA Launch cover” · page 1 · “Headline” selected. | A boxed "CURRENT VIEW" label, then the names without quotes or the word "selected". |
| UX1-69 | Agents, MCP page | A ~992px content column at a 1595px viewport; horizontal rules between sections; the URL in a bordered row. | Fixed (3f6873d): the content column is ~992px and uses section rules plus a bordered URL row. Per-app steps retain today's wording. |

### Owner decisions on the audit (2026-10-06)

The audit ([ux-batch-1-fidelity.md](ux-batch-1-fidelity.md)) found the two prototypes disagree on UX1-8: `index.html` keeps Properties as a right-hand column and Export as a popover, `chat.html` opens Export in the left flyout. The owner chose **the editor prototype**: UX1-8 is closed by UX1-4 and UX1-5 as decided above, and no shared left flyout is built for Properties or Export.

- **Wording of stopped notices (UX1-37…UX1-43): keep "computer".** Round 3 named the thing "computer", so the build's notices stay and the `chat.html` "device/Runtime" wording is not copied. The one exception is the offline-during-a-turn notice (UX1-38, UX1-66), which takes the round-3 `devices.html` wording: "{name} went offline during this turn." / "Start it on your computer, then begin a new session. To use another online computer, choose it below first." (the last sentence only when another computer is online).
- **UX1-11 stays as built.** The round-1 collapsed composer ("Ask the agent…") replaces the prototype's rail chat icon by decision.
- **H2 closed the remaining chat findings:** UX1-12, UX1-13, UX1-14, the approval helper (UX1-15, UX1-53), and the ready notice (UX1-46). The offline-during-a-turn wording (UX1-38, UX1-66) is also fixed below.

### H1 implementation evidence

UX1-4, UX1-5, UX1-9, UX1-10 and UX1-69 were implemented in `3f6873d` and browser-proved on FakeProvider instance 42.

| Finding | Status | Browser comparison |
|---|---|---|
| UX1-4 | fixed (3f6873d) | [app](evidence/ux1/H1-4-app.png) / [prototype](evidence/ux1/H1-4-proto.png) |
| UX1-5 | fixed (3f6873d) | [app](evidence/ux1/H1-5-app.png) / [prototype](evidence/ux1/H1-5-proto.png) |
| UX1-8 | closed (owner decision) | Calm keeps Properties in the right column and Export under its button. |
| UX1-9 | fixed (3f6873d) | [app](evidence/ux1/H1-9-app.png) / [prototype](evidence/ux1/H1-9-proto.png) |
| UX1-10 | fixed (3f6873d) | [app](evidence/ux1/H1-10-app.png) / [prototype](evidence/ux1/H1-10-proto.png) |
| UX1-69 | fixed (3f6873d) | [app](evidence/ux1/H1-69-app.png) / [prototype](evidence/ux1/H1-69-proto.png) |

### H2 implementation evidence

The chat header, composer and context were compared with `chat.html` at 1595×937. UX1-38 and UX1-66 were browser-proved on FakeProvider instance 71 by leaving an approval pending, stopping only that instance's test driver, and confirming the offline-during-a-turn notice; the instance was then stopped.

| Finding | Status | Browser evidence |
|---|---|---|
| UX1-12 | fixed (5987c28) | [Session info popover](evidence/ux1/H2-UX1-12-app.png) |
| UX1-13 | fixed (5987c28) | [Composer](evidence/ux1/H2-UX1-13-app.png) |
| UX1-14 | fixed (5987c28) | [Design, page and selected layer context](evidence/ux1/H2-UX1-14-app.png) |
| UX1-15 | fixed (1f8fd21) | [Approval helper](evidence/ux1/H2-UX1-15-app.png) |
| UX1-38 | fixed (5987c28) | [Offline during a turn](evidence/ux1/H2-UX1-38-app.png) |
| UX1-46 | fixed (5987c28) | [Session ready notice](evidence/ux1/H2-UX1-46-app.png) |
| UX1-53 | fixed (1f8fd21) | [Approval helper](evidence/ux1/H2-UX1-53-app.png) |
| UX1-66 | fixed (5987c28) | [Devices F offline wording](evidence/ux1/H2-UX1-66-app.png) |

## Question answered, no change

**"Opening another project showed the same agent history. Is that normal?"** Yes, by design: the chat belongs to the window, not to a project or design (`App.tsx`; CLAUDE.md invariants). Each message carries the `[forma-view]` line for where the person is when they send it, so the agent acts on the current design. A new conversation starts only with **New session** or a Run on change (U-2). A per-project history was never decided and is not in scope.
