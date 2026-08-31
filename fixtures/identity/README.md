# Test signing material

A P-256 keypair and the JWKS that publishes its public half, used by the JWT
verifier's tests (plan §111.4).

**This key is public, committed, and signs nothing real.** It exists so those
tests can mint tokens offline: a verifier that can only be tested against a live
identity provider is a verifier nobody runs the hostile cases against, and the
hostile cases are the whole point — an `alg` header of HS256, an unknown `kid`,
an expired token, a signature that does not verify.

It mirrors the shape the real provider serves — one `ES256` / `P-256` key with a
`kid`, `use: sig`, `key_ops: ["verify"]` and no `oct` entry — because a fixture
that differs from production tests a verifier we do not ship.

Never point a deployment at it.
