I tested every case of chat.html. The structure is right. Fix these, and change nothing else:

1. **Font.** The chrome falls back to Inter/system and the artboard to Ubuntu. Use Geist for sans and Geist Mono for mono, declared the same way index.html does (variable, `font-weight: 100 900`), in the chrome and on the artboard.
2. **The transcript must open at the newest entry.** When a case or state loads, the newest item is below the fold: the approval card, the visual-result card and the view-action card are all cut off. Scroll the transcript to the bottom on load and on every new entry. If the person has scrolled up, show a small "Jump to latest" control instead of yanking them down.
3. **Only the agent speaks as "Forma Agent".**
   - In Session notices, "Your message has not been sent. No work has started." appears as an agent message. With the Runtime offline, no agent said anything. That sentence belongs inside the notice; remove the agent bubble.
   - In Another window, both options give this window's own agent two messages ("This chat belongs to the window you're looking at", "Another window closed during its turn…"). That agent knows nothing about the other session. Remove those messages. Option 1 is only the one-time line on the canvas. In option 2, this window shows nothing at all, and the inset of the closed window's transcript is the only explanation.
4. **New session opens a full-screen opaque modal** with a second confirmation. No modal. Clicking "New session" *is* the choice:
   - the notice becomes "Starting a new session…";
   - a quiet divider appears in the transcript, "New session · 10:31", with the old transcript kept above it;
   - the composer comes back enabled and focused.
5. **Send is orange and enabled while the composer is empty.** Disable it while the composer is empty, as in index.html. The primary Signal fill appears only when there is text to send.
6. **The review rail shows several cases as active at once** (C, D and E all had the filled background). Exactly one case is active: Surface 2 fill, Signal left bar, Paper text. Hover is subtle and does not stay behind.
7. **View actions, "Moved your view":**
   - Remove the extra Info notice ("Validation is open for this design… Open Validation"). It repeats the card, and its button does what already happened.
   - The zoom control reads "150" with no "%", and the artboard does not change size. Show "150%" and draw the artboard at that scale. Clipping inside the canvas is fine.
8. **No design open (Context chip → Project page):** the top bar still shows Undo, Redo, "✓ Valid" and Export. Those belong to a design; hide them on the project page and the projects list.
9. **Another window, option 2:** the inset sits over the canvas, adds a scrollbar, and pushes the zoom control below the frame. Put the inset in the review rail or below the frame, outside the editor, labelled "The closed window's transcript (not visible to the person here)". The editor frame stays exactly like a normal window.
10. Re-test every case and state after the changes, in particular scrolling and focus after each action, and fix anything else that breaks.
