#!/usr/bin/env bash
# Runs exactly what CI runs (plan section 68), so a green local check means a
# green pipeline. Usage: scripts/check.sh [--rust|--web|--e2e]
#
# The end-to-end pass (plan section 67) needs a browser. A missing one is a
# setup step rather than a broken build, so it is skipped with a note and
# `pnpm e2e:install` fetches it.
set -euo pipefail

cd "$(dirname "$0")/.."

run() {
    printf '\n\033[1m==> %s\033[0m\n' "$*"
    "$@"
}

target="${1:-all}"

if [[ "$target" == "all" || "$target" == "--rust" ]]; then
    run cargo fmt --all --check
    run cargo clippy --all-targets --all-features -- -D warnings
    run cargo test --workspace
    run ./scripts/check-bindings.sh
fi

if [[ "$target" == "all" || "$target" == "--web" ]]; then
    run pnpm install --frozen-lockfile
    # Both cover apps/web and e2e/: Playwright strips types without checking
    # them, so this is the only place an e2e spec is typechecked or linted
    # before it runs. They need no browser, so unlike the walkthrough below they
    # are never skipped. The root scripts carry the filters, which is how CI's
    # own `pnpm lint` and `pnpm typecheck` steps get the same coverage.
    run pnpm lint
    run pnpm typecheck
    run pnpm test
    run pnpm build
fi

if [[ "$target" == "all" || "$target" == "--e2e" ]]; then
    if [[ -d "$HOME/.cache/ms-playwright" ]]; then
        # Reuses the binary cargo test already built and the dist pnpm build
        # just wrote, so this costs seconds rather than another build.
        run pnpm e2e
    else
        printf '\n\033[33m==> end-to-end skipped: no browser. Run pnpm e2e:install\033[0m\n'
    fi
fi

printf '\n\033[32mAll checks passed.\033[0m\n'
