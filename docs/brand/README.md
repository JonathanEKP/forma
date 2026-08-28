# Forma brand direction

[index.html](index.html) is the visual reference — open it in a browser. This file is the
same guidance in text, because that is what gets grepped.

**This is direction, not specification.** The examples in the HTML communicate hierarchy,
rhythm, semantics and visual behavior. They are not pixel specs, not a component library
and not an approved layout. Adapt dimensions, spacing and derived surface tones to
usability and accessibility; preserve the character and the semantics.

## The one distinction that matters

Forma's product identity is **not** the `BrandKit` domain type.

| | Forma brand direction (this folder) | `BrandKit` (plan §13) |
| --- | --- | --- |
| Whose brand | Forma's own, the tool | The **user's**, per project |
| Applies to | The editor chrome in `apps/web` | The designs the user produces |
| Lives in | CSS/assets in the frontend | SQLite, `brand_kits` table |
| Who chooses it | Us | The user, editable per project |

The renderer resolves `DesignColor::Token` against the *user's* palette. Forma Signal
orange must never appear in `renderer`, in `design-core`, or as a default `BrandKit`
value. If a design comes out orange, that has to be because the user's brand is orange.

Watch out for a live trap: the sample JSON in plan §13 uses `#FF5436` as `primary`, two
characters away from Forma Signal `#FF5A36`. They are unrelated — one is an illustrative
user palette, the other is our identity. Do not "unify" them.

## Color

| Token | Hex | Role |
| --- | --- | --- |
| Forma Ink | `#171717` | Primary dark environment and core structure |
| Forma Paper | `#F4F1EA` | Primary light surface, canvas, reversed foreground |
| Forma Signal | `#FF5A36` | Active, selected, transforming, primary action |
| Forma Graphite | `#62605C` | Support tone. Not for low-contrast text on Ink |

Dark-first: dark is the canonical environment, light is a deliberate alternate mode using
warm Paper rather than pure white. No blue-purple AI palettes, no cold neutrals.

**Signal is semantic, not decorative.** It marks a selected layer, a transform handle, an
agent action, a focused control, progress, or one high-value CTA. If everything is orange,
nothing is active. Surface tones derived from these four (raised panels, borders, muted
text) are suggestions — tune them for contrast.

## Typography

Geist for product UI: quiet, readable, tight tracking on display sizes. Geist Mono is the
natural companion for code and technical values. The personality comes from geometry and
construction motifs, not from expressive type.

Geist ships with the repo, in both of the places that need it and from neither a CDN:
`data/fonts/` holds the static per-weight `.ttf` the *renderer* loads (plan §34), and
`apps/web/src/fonts/` holds the variable `woff2` the editor chrome declares in
`apps/web/src/index.css`. Forma is local-first, so the UI has to look the same offline.

The two are genuinely separate concerns. Canvas text is flattened to paths by the
renderer before it reaches the browser (plan §37), so the artwork never depends on the
chrome's faces — and a font added to one side is not thereby available to the other.

The wordmark SVGs are outlined, with an alternate `a` and roughly −2% tracking. Use them
for brand placements; use live Geist text for UI.

## Assets

In [assets/](assets/). Never recreate, recolor or "improve" this geometry in product code
— reference the files.

| File | Use |
| --- | --- |
| `forma-lockup-color.svg` | Lockup on Paper / light surfaces |
| `forma-lockup-reversed.svg` | Lockup on Ink / dark surfaces |
| `forma-lockup-mono.svg` | Lockup where color is unavailable |
| `forma-symbol-color.svg` | Symbol on light |
| `forma-symbol-reversed.svg` | Symbol on dark |
| `forma-symbol-mono.svg` | Symbol, single color |
| `forma-wordmark-light.svg` | Wordmark on dark |
| `forma-wordmark-dark.svg` | Wordmark on light |

The orange node in the mark is part of the concept — the active point on a surface under
tension — not an accent that can be moved or dropped.

## Visual language

Vector-construction motifs: control points, Bézier tension, selected objects with handles.
These are motifs available when they carry meaning, not decorations owed to every screen.
The editor gets this for free: selection handles and active layers give Signal a genuinely
functional role.

## Do / Don't

Do: dark environment with warm neutral surfaces; Signal for active/selected/transforming
and primary actions; quiet typography; construction motifs used selectively; layouts
adapted to real usability.

Don't: walls of orange; purple/blue AI gradients, sparkles, glows or neural imagery;
recreated or mutated logo geometry; treating the HTML samples as mandatory specs; heavy
rounding, gloss and decoration everywhere.

## Where this applies

The editor chrome — `apps/web`. The UI mock in the HTML is consistent with the
layout of plan §10 (pages and layers left, canvas center, properties right, Preview and
Export top right), so it reads as a rendering of that spec rather than a competing one.

It does **not** apply to rendered design output, which belongs to the user's `BrandKit`.
