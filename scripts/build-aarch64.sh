#!/usr/bin/env bash
# Builds the two binaries and the editor for an Ampere A1 box (plan section 89:
# aarch64-unknown-linux-gnu, and do not assume x86).
#
# Cross-compiles from this machine rather than building on the VM: 2 OCPU is a
# slow place to compile Rust, and the VM then needs no toolchain at all.
set -euo pipefail

TARGET=aarch64-unknown-linux-gnu
OUT=${1:-dist/$TARGET}

if ! rustup target list --installed | grep -qx "$TARGET"; then
    echo "==> rustup target add $TARGET"
    rustup target add "$TARGET"
fi

# Two dependencies compile C -- libsqlite3-sys and zstd-sys -- so a rustup
# target is not enough on its own; something has to cross-compile and link for
# aarch64. Either toolchain does.
if command -v aarch64-linux-gnu-gcc >/dev/null; then
    export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
elif command -v zig >/dev/null; then
    # zig ships a cross toolchain for every target it knows, so this needs no
    # root. Pinning the glibc floor at 2.35 keeps the binary usable on anything
    # from Ubuntu 22.04 up; the VM is on 24.04.
    SHIM=$(mktemp -d)
    trap 'rm -rf "$SHIM"' EXIT
    # cc-rs appends its own --target=<rust triple>, which zig does not parse, so
    # the shim drops it.
    cat > "$SHIM/cc" <<'SHIM_CC'
#!/bin/sh
args=""
for a in "$@"; do
  case "$a" in
    --target=*) ;;
    *) args="$args '$a'" ;;
  esac
done
eval exec zig cc -target aarch64-linux-gnu.2.35 $args
SHIM_CC
    sed 's/zig cc/zig c++/' "$SHIM/cc" > "$SHIM/cxx"
    printf '#!/bin/sh\nexec zig ar "$@"\n' > "$SHIM/ar"
    chmod +x "$SHIM"/*
    export CC_aarch64_unknown_linux_gnu="$SHIM/cc"
    export CXX_aarch64_unknown_linux_gnu="$SHIM/cxx"
    export AR_aarch64_unknown_linux_gnu="$SHIM/ar"
    export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER="$SHIM/cc"
    echo "==> cross-compiling with zig $(zig version)"
else
    echo "No aarch64 cross toolchain." >&2
    echo "Install one of:" >&2
    echo "  gcc-aarch64-linux-gnu   (Debian/Ubuntu, needs root)" >&2
    echo "  zig                     (no root; a single download)" >&2
    echo "or build on the VM itself." >&2
    exit 1
fi

echo "==> cargo build --release --target $TARGET"
cargo build --release --target "$TARGET" -p forma-server -p forma-mcp

echo "==> pnpm build"
pnpm build

mkdir -p "$OUT"
cp "target/$TARGET/release/forma-server" "target/$TARGET/release/forma-mcp" "$OUT/"
rm -rf "$OUT/web" "$OUT/fonts"
cp -r apps/web/dist "$OUT/web"
# The fonts travel with the build. The renderer loads only from FORMA_FONT_DIR
# and never from fonts installed on the host, which is what makes the VM render
# exactly what this machine renders.
cp -r data/fonts "$OUT/fonts"

echo
echo "Built into $OUT:"
file "$OUT/forma-server" | sed 's/^/  /'
du -sh "$OUT" | sed 's/^/  /'
echo
echo "Next: copy forma-server, forma-mcp, web/ and fonts/ to the VM and install"
echo "them per docs/deploy/oracle.md."
