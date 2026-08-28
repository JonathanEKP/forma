# Fonts

The renderer loads fonts only from this directory (plan section 34), never from
fonts installed on the host, so local machines, CI, Docker and Oracle all produce
identical renders. A CSS `@font-face` or a Google Fonts import cannot substitute
for a file here: `usvg` resolves glyphs from a `fontdb` built by
`FontRegistry::from_dir`, and nothing in the render path touches the network.

## Current contents

| File | Family | Weight | License |
| --- | --- | --- | --- |
| `DejaVuSans.ttf` | DejaVu Sans | 400 | Bitstream Vera / DejaVu |
| `DejaVuSans-Bold.ttf` | DejaVu Sans | 700 | Bitstream Vera / DejaVu |
| `Geist-Regular.ttf` | Geist | 400 | SIL OFL 1.1 |
| `Geist-Medium.ttf` | Geist | 500 | SIL OFL 1.1 |
| `Geist-SemiBold.ttf` | Geist | 600 | SIL OFL 1.1 |
| `Geist-Bold.ttf` | Geist | 700 | SIL OFL 1.1 |
| `GeistMono-Regular.ttf` | Geist Mono | 400 | SIL OFL 1.1 |
| `BodoniModa-Variable.ttf` | Bodoni Moda | 400–900 | SIL OFL 1.1 |
| `PlayfairDisplay-Variable.ttf` | Playfair Display | 400–900 | SIL OFL 1.1 |

The registry therefore exposes five families: `DejaVu Sans`, `Geist`,
`Geist Mono`, `Bodoni Moda` and `Playfair Display`. `forma_status` reports this
list, and an agent must not name a family outside it.

The licence text of every family is in `licenses/`. The OFL requires the licence
to travel with the font, and naming it in a table is not the same as shipping
it.

## Geist

Forma's own visual direction (`docs/brand/README.md`) calls for Geist. These are
Google Fonts' static per-weight builds (Geist v5, Geist Mono v6) rather than the
variable font, because a static file per weight sidesteps the question of how
reliably `usvg` selects a named instance off a variation axis. The editor chrome
is a separate concern and uses the variable `woff2` in `apps/web/src/fonts/`.

`crates/renderer/tests/geist.rs` guards the part that a file listing cannot: it
measures glyph bounds to prove the faces are actually resolved. `usvg` falls back
silently when it cannot find a family, so a design would still render — just not
in the font it asked for. The tests catch that by comparing Geist against DejaVu,
by checking that weight 400 and weight 700 lay out differently, and by checking
that Geist Mono advances uniformly.

## The display serifs

Geist is a grotesque, and a grotesque cannot make a magazine cover look like a
magazine cover. Editorial display is a Didone problem: extreme contrast between
hairline and stem, which is the texture the eye reads as "cover". The catalog had
no serif at all, so an agent asked for an editorial piece could not produce one
however well it composed.

| Family | Register | Use |
| --- | --- | --- |
| Bodoni Moda | Extreme contrast, hairlines that nearly vanish | Mastheads, one or two words, large |
| Playfair Display | Didone but less extreme, sturdier at mid sizes | Headlines of several words |

Both ship **only** as variable fonts, which cuts against the reason Geist ships
as a static file per weight. `crates/renderer/tests/display_serifs.rs` is what
makes that acceptable: it renders every letter at weight 400 and at 900 and
compares the drawn ink.

It measures ink rather than width on purpose. usvg can read the variable
*metrics* correctly — so the advance grows with weight and any width comparison
passes — while flattening an outline from the default instance. That failure is
invisible to bounds and obvious on the page: one hairline letter inside a heavy
word.

A note for whoever reads a Didone specimen next: at weight 900 the `A` still has
a hairline left diagonal against a very fat right stem, and next to `D` it can
read as though it missed the weight change. It did not. That is the letterform,
and the ink measurement says so — Bodoni's `A` gains 141% between 400 and 900,
more than any other letter.

Instrument Serif was also evaluated and works (static, one weight, a lighter and
more refined register). It is not shipped only to keep the catalog small: three
display serifs is a choice an agent has to reason about on every piece.

## Plan section 93.2 — the default font decision, settled

**The default is Geist for text and Geist Mono for code. DejaVu Sans stays as
the fallback for glyphs Geist lacks, and is never a default.**

The plan holds the decision open until four things are reviewed. All four, with
what was actually measured rather than assumed:

- **Licence.** Geist is SIL OFL 1.1, which permits redistribution and bundling,
  so committing these files is sound. DejaVu is under the Bitstream Vera
  licence, likewise redistributable.

- **Required weights.** Geist ships 400, 500, 600 and 700, plus Geist Mono at
  400. DejaVu ships 400 and 700 only. Four steps make a usable typographic
  scale; two make a headline and a body, and the jump between them is abrupt.
  This is the criterion that decides it for a design tool. The renderer cannot
  synthesise a weight it does not have — add a file here if a design needs one.

- **`usvg` compatibility.** Verified by `crates/renderer/tests/geist.rs`, which
  measures glyph bounds rather than trusting a file listing: usvg falls back
  silently, so a design would still render, just not in the font it named.

- **Visual result.** `cargo run -p renderer --example font_specimen` writes a
  sheet of both families at every shipped weight, plus the punctuation a design
  actually uses. Geist's four weights are clearly distinguishable; it is a
  modern neutral grotesque and sets tighter than DejaVu at the same size.

### Coverage, and why DejaVu stays

Geist maps **728** code points, DejaVu **5,918** — an eight-fold difference,
counted from the `cmap` tables. That sounds decisive against Geist until you
look at where the gap is:

| block | Geist | DejaVu |
| --- | --- | --- |
| Basic Latin | complete | complete |
| Latin-1 accents | complete | complete |
| Latin Extended-A | 117/128 | complete |
| Cyrillic | complete | complete |
| Greek | 6/57 | 56/57 |
| General punctuation | 14/32 | complete |

Every character a design actually reaches for is in Geist, checked one by one:
curly quotes, en and em dashes, the ellipsis, the bullet, €£¥, the degree sign,
×, ™, →, ‰, and Spanish `¿ñá`. The gap is Greek and the rarer end of Latin
Extended.

So Geist is the default. DejaVu earns its 1.5 MB as the fallback, and that is a
measured claim rather than a hopeful one: usvg falls back **per glyph** to
another family in the registry. Rendered with Geist alone, `Δοκιμή ελληνικά`
comes out as tofu boxes; with DejaVu present it reads.
`a_glyph_the_named_family_lacks_falls_back_to_one_that_has_it` in
`crates/renderer/tests/geist.rs` pins that, so nobody reclaims the megabyte and
silently turns every non-Latin design into boxes.

A user who needs Greek in their own brand still adds a font here. The fallback
is a floor, not a substitute for choosing type.

### Which face answers, and why it is not the filesystem's choice

Falling back is per glyph, and *which* face gets asked was, until CI caught it, an
accident. Two things conspired:

- `fontdb::Database::load_fonts_dir` walks the directory in `read_dir` order, which
  the filesystem decides. That is the face order.
- usvg's default fallback selector returns the **first** face holding the glyph,
  and rejects a candidate only when style, weight *and* stretch all differ — so
  DejaVu Sans **Bold** happily answered for weight-400 text.

Together they meant the same commit rendered Greek in Bold here and in Regular on
CI. Renders being identical on every machine is the entire reason these files are
committed rather than taken from the host, so both halves are fixed:
`FontRegistry::from_dir` sorts the files before loading, and `fonts::fallback_selector`
scores candidates by distance from the requested weight instead of taking the
first. A 400 layer now falls back to DejaVu Regular and a 700 layer to DejaVu Bold,
on any filesystem.

Matching the weight is a preference, not a requirement: if the only face holding a
glyph is the wrong weight, it is still used. Tofu is worse than bold.

The coverage check reuses `skrifa`, the parser usvg itself shapes with — a second
implementation could disagree about a glyph and send the text to a face that then
draws boxes anyway.

The golden renders do not protect any of this: they are Latin-only, so the fallback
never fires in them. `crates/renderer/tests/geist.rs` is what does.

## The catalog is guarded

A Brand Kit can name a family this directory does not have. `MISSING_BRAND_FONT`
reports it, but only as a warning, and both it and `FONT_NOT_FOUND` are skipped
outright when the caller supplies no font catalog — so a bad family could sit in
the shipped kit while every design using the token rendered in a fallback face.

`apps/server/src/test_card.rs` now crosses `default_brand_kit` with a real
`FontRegistry` built from this directory, using the actual validator rather than
a reimplementation of its rules. Name a family in a Brand Kit without adding the
file here and that test fails.
