The owner found a regression in chat.html, and I confirmed it. Four controls open a full-screen opaque `<dialog>` that hides the whole editor:
- the rail icons Layers and pages, Assets and Brand;
- the (i) "Open session information" button in the Agent header.

Forma has no modals, as settled in round 1. Fix it the way index.html's Calm layout already does:

1. **Rail icons open a flyout panel** next to the rail, inside the editor frame, on the left of the canvas, like index.html's "Assets" flyout. It has a header with the panel's name and a ×. Only one flyout is open at a time. Clicking the active icon again closes it, and so does Escape. The icon shows its active state while its flyout is open. The canvas stays visible and the flyout does not cover the Agent chat. Copy the markup and behaviour from index.html rather than re-inventing them.
2. **Session information is a small popover** anchored under the (i) button, inside the Agent panel: device, session started at, and "This window's session". It closes on click outside and on Escape. No backdrop.
3. Remove the `<dialog>` element entirely so that nothing can open it again.
4. Small fix: after "New session", the divider "New session · hh:mm" goes *above* the notice "Your new session is ready", because it marks where the new session starts.
5. Re-test every rail icon and the (i) button in every review case, and check that nothing in the file opens a modal.
