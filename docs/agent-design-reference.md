# Agent design reference

Detail that would bloat [the skill](../skills/forma/SKILL.md) but that is worth
having when something does not behave as expected. The skill is the contract;
this explains why parts of it are the way they are.

## Why there is no `update_layer`

The public surface is a fixed set of typed operations. There is deliberately no
`update_layer { arbitrary_json }`, because a patch aimed at the wrong layer type
would then silently do nothing, and neither the agent nor the person would know
which of the changes they sent had actually landed. A mismatched patch returns
`INVALID_OP` naming both types instead.

The same reasoning rules out per-property tools. `add_text`, `move_layer` and
`set_color` as separate tools would make a coherent change into five round
trips, five versions in the history, and five chances to be interrupted halfway.

## Why a batch is all-or-nothing

Operations apply in order to a copy of the document. The result is validated
structurally, and only then is it persisted, in a single transaction that bumps
the version once. So a batch either happened or it did not — there is no state
where half of a change is stored.

This is also why the limit is 20 rather than unlimited: it keeps a rejected
batch small enough to reason about.

## Why `base_version` exists

A human editing in the browser and an agent working over MCP are writing to the
same document. The version guard is what stops the later write from silently
discarding the earlier one:

```sql
UPDATE designs SET version = ?, … WHERE id = ? AND version = ?
```

Zero rows affected means the design moved. The server then distinguishes the two
possible reasons — `VERSION_CONFLICT` if the row exists at another version,
`DESIGN_NOT_FOUND` if it is gone — because one says refetch and the other says
stop.

Undo does not decrement the version. Restoring an older snapshot writes a *new*
version, so history only moves forward and a version number never describes two
different documents.

## Why text has no automatic wrapping

Wrapping requires a typographic engine. Forma has exactly one, in the renderer,
and a second implementation — in the agent, in the editor, or in a wrapper — 
would drift from it and produce previews that lie. Until wrapping lives in the
renderer, line breaks are explicit `\n` characters that you control.

The upside is that overflow is *measured*: `TEXT_OVERFLOW_WIDTH` comes from the
bounds of the shaped glyphs, so it is a fact rather than an estimate.

## Why an image layer is only an `asset_id`

External URLs die, change, or point somewhere the renderer cannot reach. Keeping
the reference internal means the same document renders identically on a laptop
and on a server, the bytes can be deduplicated by hash, and the licence and
provenance stay attached to the asset rather than to whichever layer used it.

It also means the renderer needs no network access while drawing, which is a
smaller attack surface than it sounds.

## Why two bounding boxes

The renderer reports both a `layout` box and a `painted` box for every layer,
and they routinely disagree. For text, the layout box spans the line boxes while
the painted box hugs the glyph outlines. For a `cover` image, the layout box is
the whole scaled image and the painted box is the frame it was clipped to.

Overflow checks compare the layout box against the frame. Canvas checks use the
painted box — otherwise every full-bleed photo would be reported as escaping the
canvas, which is exactly what it is supposed to do.

## Why severity matters

Errors block export; warnings do not. The split is deliberate rather than
uniform:

- Text leaving the canvas is an error — that is lost copy.
- A shape leaving the canvas is a warning — bleed is a technique.
- Anything entirely off-canvas is an error whatever it is, since none of it is
  visible.
- An off-brand colour literal is a warning and never a rejection, because the
  user is allowed to ask for a specific colour.

## Why the honesty rule about previews

`render_preview` returns the PNG in the tool result, but not every host shows it
to the model. An agent that says "it looks good" without having seen anything is
reporting a guess as an observation, and the person has no way to tell the
difference. Say which of the two you are doing.

## Why section 12 talks about photographs specifically

The heuristics used to be generic, and one line of them caused a defect. It read
"text over a photograph needs a real contrast solution — a scrim, a shape behind
it, or a crop with empty space", listing the scrim first. An agent asked for a
magazine cover did exactly that: two flat rectangles at 55% and 60% opacity, one
cutting across a building and one across the subject's calves. The advice was
followed faithfully and the result was wrong.

A flat fill is opaque to its own edges, so a rectangle behind text lays a visible
seam over the picture. The fix was not to add a gradient primitive — that would
have made a bad instinct easier to execute. The photograph already had two large
quiet regions, and the piece that read as a cover was the one that used them and
added nothing.

The other rules in that section come from the same two renders: a nameplate set at
60pt and centred while the rest of the piece ranged left; an accent-coloured
"n.º 12" dropped on wet asphalt where it disappeared; two text boxes sized tighter
than their own lines, which is an error rather than a cosmetic problem; and a
design reported as finished while `validate_design` still returned two of them,
because the clean validation run had described an earlier version.

None of this is medium-specific in the sense plan section 51.11 excludes. Carousel
hooks and story arcs belong in a `social-carousel` skill. Where type can sit on a
photograph is craft that a poster, a cover and a slide all need.
