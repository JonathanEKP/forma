#!/usr/bin/env bash
# Runs exactly what CI runs (plan section 68), so a green local check means a
# green pipeline. Usage: scripts/check.sh [--rust|--web|--e2e|--pontis]
#
# The end-to-end pass (plan section 67) needs a browser. A missing one is a
# setup step rather than a broken build, so it is skipped with a note and
# `pnpm e2e:install` fetches it.
#
# The Pontis sidecar (apps/pontis-sidecar) needs a local Pontis checkout, which
# CI does not have: Pontis is unpublished, and the sidecar reaches it through
# one symlink, apps/pontis-sidecar/.pontis (see its README). Without that
# checkout nothing of the sidecar can be built, linted or run, so every part of
# it is skipped and named — never passed quietly, because a green run over
# code nobody exercised is worse than a red one.
set -euo pipefail

cd "$(dirname "$0")/.."

run() {
    printf '\n\033[1m==> %s\033[0m\n' "$*"
    "$@"
}

skipped=()

skip() {
    printf '\n\033[33m==> %s\033[0m\n' "$1"
    shift
    for item in "$@"; do
        printf '\033[33m      skipped: %s\033[0m\n' "$item"
        skipped+=("$item")
    done
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
        skip "end-to-end skipped: no browser. Run pnpm e2e:install" \
            "the Playwright walkthrough (e2e/)"
    fi
fi

if [[ "$target" == "all" || "$target" == "--pontis" ]]; then
    seam=apps/pontis-sidecar/.pontis
    driver=apps/pontis-sidecar/test-driver/Cargo.toml
    # What the sidecar and its driver import or link. Pontis's TypeScript
    # packages are used from their built dist/, which is Pontis's to build.
    needed=(
        packages/capability-core/dist/index.js
        packages/sdk-server/dist/index.js
        services/control-plane/dist/index.js
        crates/runtime/Cargo.toml
        crates/providers/fake/Cargo.toml
    )
    missing=""
    if [[ ! -e "$seam" ]]; then
        missing="there is no Pontis checkout at $seam (ln -s /path/to/pontis $seam)"
    else
        for file in "${needed[@]}"; do
            if [[ ! -e "$seam/$file" ]]; then
                missing="$seam/$file is missing (build Pontis: pnpm -r build in the checkout)"
                break
            fi
        done
    fi

    if [[ -z "$missing" ]]; then
        if [[ "$target" == "--pontis" ]]; then
            run pnpm install --frozen-lockfile
        fi
        run pnpm --filter forma-pontis-sidecar lint
        run pnpm --filter forma-pontis-sidecar typecheck
        # The FakeProvider driver is a test instrument outside the workspace,
        # held to the same Rust bar as everything inside it.
        run cargo fmt --manifest-path "$driver" --check
        run cargo clippy --manifest-path "$driver" --all-targets -- -D warnings
        # The local run drives the real binaries; build both so it never runs
        # against a stale one.
        run cargo build -p forma-server
        run cargo build --manifest-path "$driver"
        run pnpm --filter forma-pontis-sidecar test
    else
        skip "Pontis sidecar skipped: $missing" \
            "lint of apps/pontis-sidecar (eslint, --max-warnings 0)" \
            "typecheck of apps/pontis-sidecar (tsc)" \
            "fmt and clippy of apps/pontis-sidecar/test-driver" \
            "unit tests of apps/pontis-sidecar (Boundary F client, capabilities, composition)" \
            "the local run against FakeProvider (read, write, cancel, approval)"
    fi
fi

if [[ ${#skipped[@]} -eq 0 ]]; then
    printf '\n\033[32mAll checks passed.\033[0m\n'
else
    printf '\n\033[32mEvery check that ran passed.\033[0m \033[33m%d did not run:\033[0m\n' "${#skipped[@]}"
    for item in "${skipped[@]}"; do
        printf '\033[33m  - %s\033[0m\n' "$item"
    done
fi
