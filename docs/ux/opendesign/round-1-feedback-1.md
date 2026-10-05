Good first pass — the content is right (named sentence, destroyed-count, chips, Show on canvas, Details, countdown). Polish pass, keep everything else:

1. Typography weight is wrong: almost everything renders semibold. In Forma's chrome body text, agent replies, chat messages, layer names and controls are 400; only the 10px uppercase labels, the card's action sentence and panel titles use 500–600. Make sure the Geist variable font is declared with `font-weight: 100 900` so weights actually resolve.
2. Remove "MCP" from the Agent panel header. This chat runs through Pontis on the person's device; the header should only say "● Connected · live" and "My laptop · online".
3. Make the card more compact: put the DESTRUCTIVE badge on the same row as "NEEDS YOUR APPROVAL" with the countdown right-aligned on that row; buttons at normal control height (~30px), left-aligned, not full-width; tighten vertical gaps to 8px. The card should fit with the preceding agent message visible in a ~720px-tall panel.
4. Distinguish the page chip from layer chips with a small page icon instead of the "(page)" suffix.
5. "Show on canvas" should visibly select the affected layers (Signal outline on the artboard elements and the Signal selected state in the Layers list), and the button text should become "Clear selection" while active.
6. Settled states: collapse to a single compact line inside the transcript, e.g. "✓ Approved · Deleted the layer “Headline”" / "Denied · The layer “Headline” was kept" / "Expired before you answered · Nothing was deleted", in muted text, no buttons.
