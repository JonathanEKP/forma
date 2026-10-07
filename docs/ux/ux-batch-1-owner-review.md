# UX batch 1 — owner review

**Run:** 2026-10-06, browser-stack instance 30, real Codex (`gpt-6-luna`, effort low), person A, on `main` at `40b40f4`. The owner used the editor and chat by hand after batch 1 was merged.

## Findings

| Id | Where | What the owner saw | Decision |
|---|---|---|---|
| UX1-4 | Editor, Properties panel | The panel has no close button. Its only exit is deselecting, which nobody guesses. | **Superseded by UX1-8.** Properties opens in the shared left flyout, whose × closes it, as do Escape and deselection. Closing does not change the selection, and selecting a layer reopens it. |
| UX1-5 | Editor, Export | The export options open, but there is no obvious way to close them. | **Superseded by UX1-8.** Export opens in the shared left flyout with its ×, and Escape returns focus to Export. No backdrop, no modal (C-5). An export already running keeps running when the flyout closes. |
| UX1-6 | Chat, transcript scrollbar | The scrollbar style is not liked. | Invisible until used: no visible bar at rest; a thin, semi-transparent overlay thumb appears while the pointer is over the transcript or it is scrolling, then fades. The content width does not shift when it appears. Keyboard and wheel scrolling are unchanged. |
| UX1-7 | Chat, Jump to latest | The text button sits in the layout between the transcript and the composer, cutting the conversation, and looks plain. | A small round floating button with a ↓ arrow, overlaid on the bottom-right of the transcript. It takes no layout space and has the accessible name "Jump to latest". When a new entry arrives while the person is scrolled up, it shows a small Signal dot. It appears and disappears with a short fade, and the follow rule of UX1-3 is unchanged. |

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

## Question answered, no change

**"Opening another project showed the same agent history. Is that normal?"** Yes, by design: the chat belongs to the window, not to a project or design (`App.tsx`; CLAUDE.md invariants). Each message carries the `[forma-view]` line for where the person is when they send it, so the agent acts on the current design. A new conversation starts only with **New session** or a Run on change (U-2). A per-project history was never decided and is not in scope.
