#!/usr/bin/env bash
# Plan section 30: Rust is the source of truth for types, and CI must fail when
# the checked-in TypeScript bindings no longer match it.
#
# ts-rs exports during `cargo test`, so regenerating is just running the tests
# and then asking git whether anything moved.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo test --workspace --quiet >/dev/null

changed="$(git status --porcelain -- bindings/generated)"
if [[ -n "$changed" ]]; then
    echo "Generated bindings are out of date:"
    echo "$changed"
    echo
    echo "Run 'cargo test --workspace' and commit bindings/generated."
    exit 1
fi

# Known limitation: a type deleted in Rust leaves its .ts file behind, and git
# sees no change. Delete the orphan by hand when you remove a type.

echo "Bindings are up to date."
