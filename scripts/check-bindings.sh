#!/usr/bin/env bash
# Plan section 30: Rust is the source of truth for types, and CI must fail when
# the checked-in TypeScript bindings no longer match it.
#
# ts-rs exports during `cargo test`, so regenerating is just running the tests
# and then asking whether that changed anything on disk.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo test --workspace --quiet >/dev/null

# Only working-tree changes count. A file that is staged but identical to what
# regeneration produced is fine: it is already correct, just not committed yet.
modified="$(git diff --name-only -- bindings/generated)"
untracked="$(git ls-files --others --exclude-standard -- bindings/generated)"

if [[ -n "$modified$untracked" ]]; then
    echo "Generated bindings are out of date:"
    [[ -n "$modified" ]] && echo "$modified" | sed 's/^/  changed:   /'
    [[ -n "$untracked" ]] && echo "$untracked" | sed 's/^/  new:       /'
    echo
    echo "Run 'cargo test --workspace' and commit bindings/generated."
    exit 1
fi

# Known limitation: a type deleted in Rust leaves its .ts file behind, and git
# sees no change. Delete the orphan by hand when you remove a type.

echo "Bindings are up to date."
