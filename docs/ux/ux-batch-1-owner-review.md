# UX batch 1 — owner review

**Run:** 2026-10-06, browser-stack instance 30, real Codex (`gpt-6-luna`, effort low), person A, on `main` at `40b40f4`. The owner used the editor and chat by hand after batch 1 was merged.

## Findings

| Id | Where | What the owner saw | Decision |
|---|---|---|---|
| UX1-4 | Editor, Properties panel | The panel has no close button. Its only exit is deselecting, which nobody guesses. | A close button (×) in the panel header, plus Escape and deselection. Closing does not change the selection the agent sees, and clicking the layer again reopens the panel. |
| UX1-5 | Editor, Export | The export options open, but there is no obvious way to close them. | Treat it as a popover: a close button (×) in its header, plus Escape (focus returns to Export) and a click outside. No backdrop, no modal (C-5). An export already running keeps running when the popover closes. |
| UX1-6 | Chat, transcript scrollbar | The scrollbar style is not liked. | Invisible until used: no visible bar at rest; a thin, semi-transparent overlay thumb appears while the pointer is over the transcript or it is scrolling, then fades. The content width does not shift when it appears. Keyboard and wheel scrolling are unchanged. |
| UX1-7 | Chat, Jump to latest | The text button sits in the layout between the transcript and the composer, cutting the conversation, and looks plain. | A small round floating button with a ↓ arrow, overlaid on the bottom-right of the transcript. It takes no layout space and has the accessible name "Jump to latest". When a new entry arrives while the person is scrolled up, it shows a small Signal dot. It appears and disappears with a short fade, and the follow rule of UX1-3 is unchanged. |

## Question answered, no change

**"Opening another project showed the same agent history. Is that normal?"** Yes, by design: the chat belongs to the window, not to a project or design (`App.tsx`; CLAUDE.md invariants). Each message carries the `[forma-view]` line for where the person is when they send it, so the agent acts on the current design. A new conversation starts only with **New session** or a Run on change (U-2). A per-project history was never decided and is not in scope.
