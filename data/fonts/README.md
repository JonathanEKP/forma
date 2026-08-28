# Fonts

The renderer loads fonts only from this directory (plan section 34), never from
fonts installed on the host, so local machines, CI, Docker and Oracle all produce
identical renders.

## Current contents

`DejaVuSans.ttf`, `DejaVuSans-Bold.ttf` — Bitstream Vera / DejaVu license
(permissive, redistributable). These are here to unblock the Phase 0 renderer
spike, and are NOT yet the product's default font.

## Open decision

Plan section 93.2 keeps the default font open until license, required weights,
`usvg` compatibility and visual result have all been reviewed. Do that review
before treating any font here as the shipped default.
