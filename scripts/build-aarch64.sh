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

# The linker is the part that is not just a rustup target. On Debian/Ubuntu:
#   sudo apt install gcc-aarch64-linux-gnu
if ! command -v aarch64-linux-gnu-gcc >/dev/null; then
    echo "aarch64-linux-gnu-gcc not found." >&2
    echo "Install it (Debian/Ubuntu: apt install gcc-aarch64-linux-gnu)," >&2
    echo "or build on the VM itself." >&2
    exit 1
fi

export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc

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
