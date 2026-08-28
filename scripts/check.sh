#!/usr/bin/env bash
# Runs exactly what CI runs (plan section 68), so a green local check means a
# green pipeline. Usage: scripts/check.sh [--rust|--web]
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
    run pnpm lint
    run pnpm typecheck
    run pnpm test
    run pnpm build
fi

printf '\n\033[32mAll checks passed.\033[0m\n'
