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

The registry therefore exposes three families: `DejaVu Sans`, `Geist` and
`Geist Mono`. `forma_status` reports this list, and an agent must not name a
family outside it.

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

## Plan section 93.2 — the default font decision

The plan keeps the shipped default open until four things are reviewed. Where
that stands:

- **License** — cleared. Geist is SIL OFL 1.1, which permits redistribution and
  bundling, so committing these files is sound.
- **Required weights** — 400, 500, 600 and 700 for Geist, 400 for Geist Mono.
  Add a file here if a design needs another; the renderer cannot synthesize one.
- **`usvg` compatibility** — verified by `crates/renderer/tests/geist.rs`.
- **Visual result** — still a human call, and the only one left.

**Adding Geist did not change any default.** `default_brand_kit` in `apps/server`
still specifies DejaVu, and DejaVu stays in this directory because that kit and a
great many tests depend on it. Switching the default is the remaining half of the
93.2 decision, not a side effect of this directory.

## The catalog is guarded

A Brand Kit can name a family this directory does not have. `MISSING_BRAND_FONT`
reports it, but only as a warning, and both it and `FONT_NOT_FOUND` are skipped
outright when the caller supplies no font catalog — so a bad family could sit in
the shipped kit while every design using the token rendered in a fallback face.

`apps/server/src/test_card.rs` now crosses `default_brand_kit` with a real
`FontRegistry` built from this directory, using the actual validator rather than
a reimplementation of its rules. Name a family in a Brand Kit without adding the
file here and that test fails.
