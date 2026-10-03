#!/usr/bin/env bash
# Plan section 30: Rust is the source of truth for types, and CI must fail when
# the checked-in TypeScript bindings no longer match it.
#
# The same holds for the agent skill. skills/forma/SKILL.md is generated from
# crates/agent-catalog/manual/SKILL.md (Pontis plan section 6.1) and committed,
# because hosts read it from that path; one check covers both, so the two never
# disagree about what counts as drift.
#
# ts-rs exports during `cargo test`, and so does the skill, so regenerating is
# just running the tests and then asking whether that changed anything on disk.
set -euo pipefail

cd "$(dirname "$0")/.."

# A failing run still regenerates, and drift is reported before the failure:
# a hand-edited skill fails skill_matches_the_code too, because the test was
# compiled against the edit and then read the regenerated file, and the drift is
# the explanation worth reading first.
tests=0
cargo test --workspace --quiet >/dev/null || tests=$?

# Only working-tree changes count. A file that is staged but identical to what
# regeneration produced is fine: it is already correct, just not committed yet.
generated=(bindings/generated skills/forma/SKILL.md)
modified="$(git diff --name-only -- "${generated[@]}")"
untracked="$(git ls-files --others --exclude-standard -- "${generated[@]}")"

if [[ -n "$modified$untracked" ]]; then
    echo "Generated files are out of date:"
    [[ -n "$modified" ]] && echo "$modified" | sed 's/^/  changed:   /'
    [[ -n "$untracked" ]] && echo "$untracked" | sed 's/^/  new:       /'
    echo
    echo "Run 'cargo test --workspace' and commit bindings/generated and skills/forma/SKILL.md."
    echo "The skill is generated: edit crates/agent-catalog/manual/SKILL.md, never the output."
    exit 1
fi

if (( tests != 0 )); then
    echo "cargo test --workspace failed (exit $tests); the generated files did not drift."
    exit "$tests"
fi

# Known limitation: a type deleted in Rust leaves its .ts file behind, and git
# sees no change. Delete the orphan by hand when you remove a type.

echo "Bindings and the generated skill are up to date."
