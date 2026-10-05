# Design System: Forma Editor

> Category: Productivity & Creative Tools
> The chrome of Forma, a local design editor driven by AI agents. Dark-first, warm-neutral, quiet; one orange that means "active".

## 1. Visual Theme & Atmosphere

Forma is a design runtime: a person and an AI agent edit the same design, and the agent's work happens in a chat panel beside the canvas. The chrome must recede so the artwork reads, and must stay calm while an agent is working.

- **Visual style:** precise, quiet, tool-like. Personality comes from geometry and vector-construction motifs (control points, Bézier handles, selected objects), used only where they carry meaning.
- **Color stance:** dark environment with warm neutral surfaces. Light mode, if ever, is warm Paper, never pure white.
- **Never:** purple/blue AI gradients, sparkles, glows, neural imagery, walls of orange, heavy rounding, gloss.

## 2. Color

- **Ink:** `#171717` — app background, core structure.
- **Paper:** `#F4F1EA` — primary text on dark; light canvas surface; reversed foreground.
- **Signal:** `#FF5A36` — semantic, not decorative: selected layer, transform handle, an agent action in progress, focused control, progress, the single primary CTA of a view. If everything is orange, nothing is active.
- **Graphite:** `#62605C` — support tone. Never low-contrast text on Ink.
- **Surface:** `#1E1E1D` — panels.
- **Surface 2:** `#252524` — raised elements inside panels (cards in the chat).
- **Canvas well:** `#20201F` — the area around the artboard.
- **Edge:** `rgba(244,241,234,0.10)` — hairline borders.
- **Edge strong:** `rgba(244,241,234,0.18)` — card borders, emphasized dividers.
- **Muted text:** `#AAA79F` — secondary text.
- **Faint text:** `#77746E` — tertiary text, metadata, technical ids.

There is no separate danger red in the chrome today. Destructive emphasis has been done with a Paper-on-Ink inverted badge ("DESTRUCTIVE"); proposing a restrained danger tone is allowed if it stays warm and does not compete with Signal.

## 3. Typography

- **Families:** sans = Geist; mono = Geist Mono (technical values, ids, capability names).
- **Scale used in the chrome:** 10px uppercase labels (tracking 0.1em, semibold) · 11px metadata · 12px controls and body in panels · 13px primary sentences in cards · 14–15px titles.
- **Weights:** 400, 500, 600. Tight tracking only on display sizes.
- Typography is quiet; hierarchy comes from size, weight and the muted/faint tones, not from color.

## 4. Spacing & Grid

- 4px base; panels pad 12px; cards pad 12px with 8px internal gaps.
- Dense but breathable: this is a professional tool used for hours.

## 5. Layout & Composition

Editor layout (fixed by the product plan):

- Top bar: back to Projects, design name and version, zoom, Export, Undo, then panel toggles (Agent, Properties, Validation, Assets, Brand).
- Left column (~220px): Pages, then Layers (name + type).
- Center: the canvas well with the artboard.
- Right: Properties panel, and the **Agent panel (~340px)** with: device/session status header, the chat transcript, and the composer (textarea + Send + Stop) at the bottom.

## 6. Components

- **Primary button:** Signal background, Ink text, semibold 12px, radius 8px. One per view.
- **Secondary button:** Surface background, Edge border, Muted text that turns Paper on hover.
- **Toggle buttons in the top bar:** pill-ish, Edge border; active state filled with Signal.
- **Chat cards** (tool activity, results, approvals, UI actions): Surface 2 background, Edge strong border, radius 8px, 12px padding, a 10px uppercase muted label as the heading.
- **Badges:** 10px uppercase semibold; heavy risk = Paper background + Ink text; light risk = Edge border + Muted text.
- **Technical values** (ids, capability names, versions): Geist Mono 11px, Faint.
- **Status line:** small dot + muted text ("Connected · live").

## 7. Motion & Interaction

- Short, purposeful transitions (120–200ms), no bounce.
- Signal is the interaction signal: focus rings, active states, progress.
- Every state explicit: hover, focus-visible, active, disabled, loading, and — specific to agent work — pending, sending, confirmed, refused, expired.

## 8. Voice & Brand

- Plain English, sentence case, short. Say what happened and what the person can do.
- Name things by what the person knows (layer names, design names, page numbers), never by internal ids unless asked.
- Never claim success nobody saw: "Not confirmed by your device" beats a fake "Done".
- Logo assets exist as SVG (lockup, symbol, wordmark; color/reversed/mono). The orange node in the mark is part of the concept. Never recreate or recolor the geometry.
