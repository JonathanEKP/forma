Prototype **the Agent chat of Forma's editor** as an interactive single-page prototype in a new file, `chat.html`. Do not change `index.html`. Use the selected Forma Editor design system and the **Calm** layout from `index.html`:
- a minimal top bar (logo, "Projects /", design name, version, Undo/Redo, validation chip, Export);
- a slim icon rail on the left;
- the canvas as the room;
- the agent docked as a chat on the right (~360px).

The chat is the subject, so make it the focus and keep the editor as quiet context. Reuse the approval card from `index.html` as it is, if one appears.

## What the chat must show (switchable from a review rail outside the frame, like index.html)

**A. Formatted agent text.** Today agent replies are plain text, so `**bold**` and `- lists` show their markers. Show a small, safe subset:
- paragraphs, bold, italic and `inline code`;
- code blocks in Geist Mono;
- bulleted and numbered lists;
- links: only http(s), which show their host and open a new tab.

Anything else, such as raw HTML or images in markdown, shows as literal text. Include a sample reply that uses all of these, and one with hostile content (`<img onerror>`, `<script>`) shown harmlessly as text.

**B. UI action cards.** These record the agent moving the person's view. The heading says the outcome:
- "Moved your view": opened “ALPHA Square post”, selected “Title”, zoom 150%, opened the Validation panel.
- "Couldn't move your view": the reason in words, e.g. "Open a design first" or "That layer isn't on this page".
- "Not confirmed by this window": nothing acknowledged it in time.
- "The agent asked for something Forma can't do": a refused request, e.g. "zoom is limited to 400%" or "there is no Settings panel".

No internal ids except inside a collapsed "Details". Long unbroken text, such as a URL or a hash, wraps. The transcript never scrolls horizontally.

**C. Session notices.** One component, three tones: info, needs you, stopped. Each notice has:
- one line saying what happened;
- one line saying what to do;
- the single action that applies.

While a notice is "stopped", the composer is disabled and says why. Nothing starts by itself. Cases, in plain words:
1. Runtime offline before sending: "Your Runtime is offline. Start it on your device, then try again." (needs you, no button)
2. Runtime went offline during a turn: stopped, **New session**.
3. Device revoked: stopped, **Open Agents page**, then New session.
4. No device paired: stopped, **Open Agents page**.
5. Runtime restarted or replaced: stopped, **New session**.
6. Stopped from another window: stopped, **New session**.
7. Sign-in expired: stopped, **Sign in again**.
8. More than one device online: needs you, "Leave one connected", no button.
9. The session ended (it timed out): stopped, **New session**.

Show where each notice sits in the transcript after the last message, and how the composer looks.

**D. Context chip.** Above the composer, the chat shows what the agent will be told the person is looking at:
- with a design open: “ALPHA Launch cover” · page 1 · “Headline” selected;
- with no design open, on a project page: Project “Launch”;
- on the projects list: nothing.

The chip is read-only. Include a state for each.

**E. An agent action that reaches a different window.** Forma can have two windows. Normally the agent moves only the window whose chat it belongs to. One edge case: that window was closed mid-turn and one other window is open. Make this switchable between two options, both shown from the *other* window's point of view:
- **Option 1, allowed:** the action lands. The window shows a one-time quiet line at the top of the canvas, "An agent from another window opened this design", with Dismiss.
- **Option 2, refused:** nothing moves in this window. The agent's chat (shown as a small inset of the closed window's transcript) says "I can't reach your window. Open the editor and ask again."

**F. A reserved slot for a visual result.** Show an image result card in the transcript: a rendered preview thumbnail, 4:5 portrait, the design name, "v7" and "Open in editor". Label it in the review rail as "future, not built yet".

## Constraints

- Same as index.html. Signal orange only for the primary action, focus, selection and urgent state. No gradients, glows, sparkles or purple/blue.
- Panel text is 12–13px, radius 8px and borders are hairline and warm.
- Plain English in sentence case, no internal ids outside Details.
- Accessible names on every button.
- Short transitions (≤150ms) that respect prefers-reduced-motion.
- Switching a review-rail option resets the transcript to that case's initial state.
