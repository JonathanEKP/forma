# UX round 2: the chat

**2026-10-05.** Second UX round, after [round 1](round-1-approval-card.md) closed with the approval card and the
Calm editor. Prototyped in OpenDesign, in a new conversation of the same project and in a new file, `chat.html`,
against the [Forma editor design system](opendesign/forma-editor-DESIGN.md) and inside the Calm layout. As in round 1,
**decisions are recorded here**; OpenDesign is the sketchpad.

Pairing and the Devices page (F-10-5, F-10-6, F-10-9) are a separate round (round 3), because they are a page of their
own and not the chat.

## The problems (browser findings)

- **F-chat-15:** agent text is never formatted. Replies render as plain `whitespace-pre-wrap`, so `**bold**` and `-`
  lists show their markers. The safety it has must be kept: agent output and hostile input never render as HTML.
- **F-40-14:** the chat's view of a UI action is thin and contradicts itself.
  - A card titled "MOVED YOUR VIEW" is shown for a call that was rejected ("didn't land" in its last line).
  - The agent pastes raw ack JSON with internal `clientId`s into its sentences.
  - A schema refusal from the Runtime reaches the person as two JSON blobs, with no card.
  - Long unbroken text is clipped behind a white, unstyled horizontal scrollbar.
- **Session notices:** the chat already says when a session cannot be carried on and offers **New session**. The current
  strings are in `apps/web/src/pontis/chat.ts` (Runtime offline, device revoked, no device paired, replaced Runtime,
  stopped from another window, sign-in expired, more than one device online). They are correct and read as a wall of
  text. Nothing starts by itself (CLAUDE.md), and the five-minute death of a session is Pontis's F-30-5, which is not
  masked here.
- **F-chat-17 (info):** with no design open, the view line is `[forma-view] none`, so the project the person is
  looking at is not part of the view and the chat shows no context chip.
- **D-2, the remaining question.** Agent actions go to the window whose chat started the session; with several windows
  and none bound, the answer is `ui_unavailable`. But Pontis's rule 2 sends an action to the **only** connected window
  even when that window's chat is not the session's. This happens when the bound window closed mid-turn and one other
  stays open. The owner decides whether that is acceptable and what the receiving window shows.

## Starting recommendations (to be validated visually)

1. **Formatted text, a small safe subset.** Paragraphs, bold, italic, inline code, code blocks, bulleted and numbered
   lists, and links. Only `http(s)` links are clickable; they open a new tab (`rel="noopener noreferrer"`) and show
   their host. Rendered from a parser into
   React nodes, never through `innerHTML`. Anything else shows literally.
2. **UI action cards say what happened.**
   - Heading by outcome: "Moved your view" only when `handled`. Otherwise "Couldn't move your view" with the typed
     reason in words ("Open a design first", "That layer isn't on this page"), or "Not confirmed by this window".
   - The card names the thing in the person's words ("Opened “ALPHA Square post”", "Selected “Title”", "Zoom 150%").
   - No ids anywhere outside a Details disclosure.
   - Long text wraps (`overflow-wrap: anywhere`); no horizontal scroll in the transcript.
   - A schema refusal shows as a quiet card ("The agent asked for something Forma can't do: zoom is limited to
     400%"), not as JSON.
3. **Session notices as one component.** One line of what happened, one line of what to do, and the one action that
   applies: New session, Sign in again, or Open Agents page. The notice is placed in the transcript where it happened,
   and the composer is disabled with the reason. It comes in three tones only: info, needs you, stopped.
4. **The context chip with no design open** names the project ("Project: Launch"). This needs F-chat-17 changed in the
   sidecar: the `[forma-view]` line carries `project=… name="…"` when a project is selected. Recommended.
5. **D-2, a recommendation.** Keep Pontis's rule 2, but the receiving window says so: a one-time line at the top of its
   view, "An agent from another window opened this design", with a link to that chat if it is still open. The
   alternative is a Forma-side refusal: the sidecar would answer `ui_unavailable` when the only window is not the bound
   one. It is stricter, the agent can say "I can't reach your window", and nothing moves unexpectedly. Both are
   switchable in the prototype.
6. **Room for the visual loop.** Leave a slot for an image result in the transcript (a rendered preview, 1:1 or
   portrait, with "Open in editor"), shown as "coming later". It waits for Pontis CR-2.3 and is not built.

## Decisions

*(filled in as the round is reviewed)*
