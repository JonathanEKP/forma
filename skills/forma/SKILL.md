---
name: forma
description: >
  Design graphics with Forma, a local design runtime driven over MCP. Use when
  asked to make a post, carousel, poster, cover or any laid-out visual piece in
  a Forma project, or when a Forma tool is available and the user mentions a
  design, a Brand Kit or an asset.
---

# Forma

Forma is a local design runtime. **You make the design decisions; Forma executes
them deterministically and renders them.** It integrates no image generation and
no model API of its own, so anything a design needs that does not exist yet is
something you ask a person for.

## 1. Mental model

You are editing a structured document, not writing HTML or CSS. There is a fixed
set of typed operations; there is no way to send arbitrary markup, and no
per-property tool. One batch of operations is one atomic change and one version.

```
project ──── brand kit        colours, type, voice, rules
   │
   ├──────── assets           images a person uploaded
   │
   └──────── designs
                └── document
                      ├── format        one canvas for every page
                      └── pages[]
                            ├── background
                            └── layers[]   order = z-order, last is on top
```

Three rules that govern everything else:

- **Ids are the only handles.** Use the ids the server returned. Never select a
  layer by its position or its text, and never invent an id for something that
  already exists.
- **The server is the authority.** After every mutation it returns the new
  record. Use that, not your memory of what you sent.
- **Read before you write.** `get_brand` before choosing any value, and
  `list_assets` before asking for an image.

## 2. DesignRecord vs portable DesignDocument

```
DesignRecord            what the server stores
├── id, project_id      identity
├── version             what you pass as base_version
└── document            portable
      ├── schema_version
      ├── format
      └── pages[]
```

The **document** carries no ids of the workspace, no version, no URLs and no
file paths — only `asset_id` references and `$brand.*` tokens. That is what lets
the same document become a template or move between projects. Do not try to put
a URL or a path anywhere inside it.

## 3. Format, pages and z-order

The format belongs to the **whole design**, not to a page: every page of a
carousel shares one canvas.

| preset | size |
| --- | --- |
| `instagram_portrait` | 1080 × 1350 |
| `square` | 1080 × 1080 |
| `story` | 1080 × 1920 |
| `landscape` | 1920 × 1080 |
| `custom` | your `width` and `height` |

A design starts with one page. Add pages with `add_page` for a carousel.

Inside a page, **layer order is the z-order**: the last layer in the array is
drawn on top. `reorder_layer` takes the target index, where 0 is the bottom.

Coordinates may fall outside the canvas. That is deliberate — a photo can bleed
off the edge. Text that leaves the canvas is lost copy, and validation treats
the two differently.

## 4. Brand tokens, voice and the font catalog

`get_brand` returns the kit and the font catalog together. Read it before you
write copy or pick a value.

Colour tokens:

```
$brand.primary     $brand.secondary   $brand.accent      $brand.background
$brand.surface     $brand.text        $brand.textInverse $brand.muted
```

Font tokens: `$brand.display`, `$brand.body`, `$brand.mono`.

Rules:

- **Use a token for any value you choose yourself.** A literal like `#FF5436` is
  allowed and is kept when the user asked for that exact colour, but a literal
  you invented earns a `COLOR_OUTSIDE_BRAND` warning.
- **`voice` is an instruction from the user.** It outranks your editorial
  defaults. If it says no hype, write no hype.
- **Never name a font outside the catalog.** The renderer cannot substitute one
  it does not have; you get `FONT_NOT_FOUND` and the design will not export.
- **If the project has no Brand Kit, `$brand.*` tokens do not resolve.** Use
  colour literals and a font from the catalog instead.

## 5. Layers and their valid fields

Every layer carries these, flattened onto the variant:

```
id, name, role?, visible, locked, opacity, transform{x,y,width,height,rotation_deg}
```

`role` is optional metadata: `headline`, `kicker`, `body`, `cta`, `stat`,
`photo`, `logo`, `decoration`. It changes nothing visually, but it lets a person
and a later template understand what a layer is for. Set it.

**text**

```
text, font_family, font_size, font_weight, line_height,
letter_spacing, align (left|center|right), color
```

There is **no automatic word wrap**. Line breaks are literal `\n` characters in
`text`, and you decide where they fall. `line_height` is a multiplier.

**image**

```
asset_id, fit (contain|cover|fill), focal_point{x,y}
```

`cover` fills the frame and crops; `contain` fits the whole image inside and
leaves gaps; `fill` stretches and distorts. `focal_point` is a fraction from 0
to 1 that decides what survives a `cover` crop — `{x:0.5,y:0.5}` crops evenly.

**shape**

```
kind (rectangle|ellipse|line), fill?, stroke?, stroke_width, corner_radius
```

`corner_radius` applies to rectangles only.

## 6. Assets and AssetRequest

An image layer holds an `asset_id` and nothing else:

```
image layer  →  asset_id: "…"          correct
image layer  →  url: "https://…"       impossible, and never try
```

When the design needs an image:

1. `list_assets` first. Reusing what exists beats waiting for a person.
2. If nothing fits, `request_asset` with a purpose, a description and a
   suggested prompt. **Forma generates no images**; the prompt is text for a
   human to take to whatever tool they prefer.
3. Carry on with everything that does not depend on that image.
4. `get_asset_request` to see whether it is `resolved`. Until then it is
   `pending` and there is no asset id to use.

Never invent an asset id, and never claim to know what an image shows if you
have not been shown it.

## 7. MCP tools

Read: `forma_status`, `list_projects`, `get_project`, `get_brand`,
`list_assets`, `get_design`, `get_asset_request`.

Write: `create_design`, `apply_operations`, `request_asset`.

Output: `validate_design`, `render_preview`, `export_design`.

There is no tool to create a project or edit a Brand Kit. Those belong to the
person; if the project or the kit you need does not exist, say so and ask.

`forma_status` also reports `max_ops_per_batch` and the font catalog, so you can
size your batches and check a family without guessing.

## 8. apply_operations and base_version

Every edit is one call:

```json
{
  "design_id": "…",
  "base_version": 4,
  "operations": [ … ]
}
```

**`base_version` is the version you actually read**, from `get_design` or from
the previous mutation's result. Not a guess, not a number you incremented
yourself.

The batch is **atomic**: the operations apply in order to a copy, the result is
checked, and only then is it saved. If one operation is rejected, none of them
happen and the version does not move. So group a coherent change into one call
rather than sending five — and expect all-or-nothing, not partial success.

At most **20 operations per batch** (`max_ops_per_batch`).

The thirteen operations:

| `op` | fields |
| --- | --- |
| `add_page` | `page`, `index?` |
| `remove_page` | `page_id` |
| `reorder_pages` | `page_ids` — the complete new order |
| `set_background` | `page_id`, `color` (null clears it) |
| `add_layer` | `page_id`, `layer`, `index?` |
| `remove_layer` | `layer_id` |
| `move_layer` | `layer_id`, `frame` — any of x, y, width, height, rotation_deg |
| `reorder_layer` | `layer_id`, `index` |
| `set_text` | `layer_id`, `content`, `font_size?` |
| `patch_text_style` | `layer_id` + any text style field |
| `set_image` | `layer_id`, `asset_id`, `fit?`, `focal_point?` |
| `patch_shape_style` | `layer_id` + any shape style field |
| `set_format` | `format` |

Things that are errors rather than no-ops:

- A patch aimed at the wrong layer type — `set_text` on a shape is
  `INVALID_OP`, naming both types.
- An empty patch, or an empty batch. Both express no intent.
- `reorder_pages` with fewer pages than exist.
- A value out of range. Nothing is silently clamped: a zero `font_size` or a
  negative width is rejected, so what you sent and what was stored never differ.

## 9. Validation

`validate_design` checks the document against deterministic rules and returns
every issue at once — it does not stop at the first.

```json
{ "valid": true, "issues": [] }
```

Each issue has `code`, `severity`, `page_id?`, `layer_id?`, `message` and
`details`. **Errors block export; warnings do not.** `valid` is false when any
error is present.

Text overflow is *measured* from the rendered glyphs, not estimated, so
`TEXT_OVERFLOW_WIDTH` means the text genuinely does not fit — shorten it, break
the line, reduce `font_size`, or widen the box.

`TEXT_LOW_CONTRAST` is measured the same way: the renderer samples what is drawn
under the line and compares it with the ink, by lightness *and* by colour. An
accent that breaks by hue passes even when the two are the same lightness. It is
an **error** when the two are near enough to be the same thing, because that is
copy the reader never gets, and a **warning** when it is merely thin or when the
line reads on average while most of its own area does not — a headline half on
dark road and half on a bright stripe. The fix is almost never a band across the
photograph: move the line, or take it to a near-white or near-black.

Do not export a design with errors. Warnings are a judgement call: a bleeding
photo warns and is fine; an off-brand literal warns and may be exactly what the
user asked for.

A judgement call is one you **state**. If you finish with warnings standing, name
them and say why each one is right — "the scrim bleeds past the edge on purpose"
is a decision; silence is indistinguishable from not having read them. That goes
double for `OUTSIDE_SAFE_AREA`, which is the Brand Kit's own rule: the person set
that margin, and stepping over it without a word is overruling them quietly.

**Validate after your last edit, not before it.** Validating, then nudging a
size or a box, then calling the piece finished is how an invalid design ships:
the run that came back clean was describing an earlier version. Every batch you
apply invalidates the last answer. The final call before you report the piece
done is `validate_design`.

## 10. Preview and visual inspection

`render_preview` returns a structured result **and** the PNG as an image block.

- **If you can see the image**, look at it and say what it actually shows.
  Check the hierarchy, the crop, the contrast of text over photography, and
  whether anything collides.
- **If your host did not deliver the image to you**, say so. Report that the
  mechanical checks passed and leave the visual judgement to the user.

Mechanical validation and visual approval are different things. Never claim to
have looked at a preview you were not given.

## 11. Error codes and recovery

Every failure carries a `code` and a `hint`. Branch on the code.

| code | what it means | what to do |
| --- | --- | --- |
| `VERSION_CONFLICT` | someone else changed the design | `get_design`, rebuild your operations against the version it returns, send again. **Never resend the same `base_version`.** |
| `DESIGN_NOT_FOUND` | the design is gone | stop; do not retry. List designs and ask the user. |
| `PROJECT_NOT_FOUND` | the project is gone | stop; `list_projects` and ask. |
| `PAGE_NOT_FOUND` | no such page in this design | `get_design` and use an id it returned. |
| `LAYER_NOT_FOUND` | no such layer | `get_design`; ids are the only handles. |
| `DUPLICATE_LAYER_ID` | you reused an id | generate a fresh one for a new layer. |
| `INVALID_OP` | the operation cannot mean anything | read the message: usually a patch on the wrong layer type, an empty patch, or an index out of range. Fix the operation; do not resend it unchanged. |
| `INVALID_DOCUMENT` | the result would be structurally broken | e.g. removing the last page. Rethink the batch. |
| `INVALID_BODY` | the request did not match the schema | a field is missing, misspelled or the wrong type. |
| `LIMIT_EXCEEDED` | more than 20 operations | split into several batches. |
| `INVALID_DIMENSION` / `INVALID_OPACITY` | a value is out of range | correct the value; nothing is clamped for you. |
| `UNKNOWN_BRAND_TOKEN` | the token does not resolve | `get_brand` for the real list, or use a literal. If the project has no kit, use literals. |
| `FONT_NOT_FOUND` | the renderer cannot draw that family | use one from the catalog in `get_brand`. |
| `ASSET_NOT_FOUND` | no such asset | `list_assets`; never invent an id. If it does not exist, `request_asset`. |
| `RENDER_FAILED` / `STORAGE_ERROR` | the server failed, not your design | do not "fix" the design. Report it. |
| `SERVER_UNREACHABLE` | forma-server is not running | tell the user to start it. Retrying will not help. |

The failure mode to avoid is the blind retry: sending the same call again after
an error that will produce the same error. Every row above changes what you send
next.

## 12. Design heuristics

- One idea per page. A carousel slide that says two things says neither.
- Clear hierarchy: headline, then supporting copy, then metadata. Make the size
  difference obvious rather than subtle.
- Respect the safe margin from the Brand Kit's rules for anything that must be
  read.
- Break lines yourself, at meaningful points, since there is no auto-wrap.
- Do not invent facts, statistics, names, prices or claims. If the user has not
  given you the copy, use theirs or ask.
- Do not invent schema fields. If something is not in section 5, the renderer
  cannot draw it.

### Type on a photograph

A photograph already has quiet regions — sky, road, a wall, anything large and
tonally even. **Find them in the render and put the type there.** That is what
makes a cover look photographed rather than captioned.

A scrim is the last resort, and reaching for it first is the most common way to
make a piece look machine-made. A flat rectangle is opaque to its own edges, so it
lays a visible seam across the picture, and a seam that crosses the subject is
worse than the contrast problem it solved.

So do it in this order: **place the type, render, and look.** If it reads, you are
done and the photograph is intact. If one element fails, fix that element — move
it, recolour it, or re-frame the photo with `focal_point`. Only when the page has
no workable region at all does a scrim earn its place, and then it covers what it
must rather than the full width out of caution.

Insurance is not free here. A band added "to be safe", behind type that would have
read anyway, costs the picture and buys nothing.

Contrast is checked, not assumed. `validate_design` measures every line against
what is behind it, so a small label dropped on a mid-toned part of a photo comes
back as `TEXT_LOW_CONTRAST` rather than reaching the reader as a grey smudge.

Mid tones are the trap. `$brand.muted` is defined against `$brand.background`,
where it has all the contrast it needs; over a photograph of the same lightness
it has none. Over photography the reliable inks are the near-white and the
near-black, and an accent used deliberately.

### Scale

Display type on a cover or poster is meant to be decisive. A nameplate set small
and centred reads as a caption, not as a masthead — if you are unsure between two
sizes, the larger one is almost always right. Hierarchy comes from the gap between
sizes, so a headline two steps above its deck says more than one a step above.

Keep the register of each face. A display serif is for display; metadata, credits
and labels belong in the body face. Setting a high-contrast serif small makes its
hairlines disappear.

### Boxes

A text box does not grow to fit, and text that does not fit is an **error**, not a
cosmetic issue. Give a box more height than you think the lines need — the cost of
a generous box is nothing, and `TEXT_OVERFLOW_HEIGHT` costs you a round trip. When
it does overflow, prefer making the box taller over making the type smaller: you
chose that size for a reason.

### Placement

Everything on the page belongs to a structure. An element parked on its own, far
from the margin the rest of the piece aligns to, reads as forgotten rather than as
deliberate — put it in relation to something, or give it a reason to be alone.

## 13. A full session

The user says: *"Make a square post announcing our beta, using the branding."*

```
get_brand { project_id }
  → colours, voice "Direct, technical and brief.", fonts ["Geist", …]

list_assets { project_id }
  → []                      nothing to reuse; this piece works with type alone

create_design { project_id, name: "Beta announcement", preset: "square" }
  → id "d1", version 1, one page "p1"

apply_operations {
  design_id: "d1", base_version: 1,
  operations: [
    { op: "set_background", page_id: "p1", color: "$brand.background" },
    { op: "add_layer", page_id: "p1", layer: {
        type: "text", id: "<fresh uuid>", name: "Headline", role: "headline",
        visible: true, locked: false, opacity: 1,
        transform: { x: 90, y: 300, width: 900, height: 400, rotation_deg: 0 },
        text: "THE BETA\nIS OPEN", font_family: "Geist", font_size: 130,
        font_weight: 700, line_height: 1.05, letter_spacing: 0,
        align: "left", color: "$brand.text" } },
    { op: "add_layer", page_id: "p1", layer: {
        type: "shape", id: "<fresh uuid>", name: "Rule", role: "decoration",
        visible: true, locked: false, opacity: 1,
        transform: { x: 90, y: 760, width: 260, height: 14, rotation_deg: 0 },
        kind: "rectangle", fill: "$brand.primary",
        stroke: null, stroke_width: 0, corner_radius: 7 } }
  ]
}
  → version 2
```

Meanwhile a person moved a layer in the editor. The next call fails:

```
apply_operations { design_id: "d1", base_version: 2, operations: [ … ] }
  → VERSION_CONFLICT  expected 2, actual 3
```

Recover by rebasing — not by resending:

```
get_design { design_id: "d1" }
  → version 3, and the headline is now at y 280

apply_operations { design_id: "d1", base_version: 3, operations: [ … ] }
  → version 4
```

Then check, look, and ship:

```
validate_design { design_id: "d1" }
  → valid false: TEXT_OVERFLOW_WIDTH on the headline, needs 980px in a 900px box

apply_operations { design_id: "d1", base_version: 4,
                   operations: [{ op: "set_text", layer_id: "…",
                                  content: "THE BETA\nIS OPEN", font_size: 116 }] }
  → version 5

validate_design { design_id: "d1" }   → valid true

render_preview { design_id: "d1" }
  → structured result + PNG
  → if you can see it: say what it shows, and fix what is wrong
  → if you cannot: say the checks passed and leave the look to the user

export_design { design_id: "d1" }
  → exported version 5
```
