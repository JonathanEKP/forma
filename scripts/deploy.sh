#!/usr/bin/env bash
# Builds, stages and installs Forma on the remote VM. Run from the repo root.
#
#   ./scripts/deploy.sh ubuntu@forma.hintly.online [-i ~/.ssh/id_rsa2]
#
# Everything it does on the far side is in deploy/install.sh, which is what
# actually runs there. This half only builds, copies, and calls it.
set -euo pipefail

HOST=${1:-}
shift || true
SSH_ARGS=("$@")
STAGE=/tmp/forma-deploy
OUT=dist/aarch64-unknown-linux-gnu

[ -n "$HOST" ] || { echo "usage: $0 user@host [ssh args...]" >&2; exit 1; }
say() { printf '\n==> %s\n' "$*"; }

say "building for aarch64"
./scripts/build-aarch64.sh "$OUT"

say "staging to $HOST:$STAGE"
# shellcheck disable=SC2029
ssh "${SSH_ARGS[@]}" "$HOST" "rm -rf $STAGE && mkdir -p $STAGE"
scp "${SSH_ARGS[@]}" -q -r \
    "$OUT/forma-server" "$OUT/forma-mcp" "$OUT/web" "$OUT/fonts" \
    deploy/install.sh deploy/backup.sh \
    deploy/forma.env.example deploy/domain.env.example \
    deploy/caddy deploy/systemd \
    "$HOST:$STAGE/"

say "installing (sudo on the far side)"
ssh "${SSH_ARGS[@]}" -t "$HOST" "sudo bash $STAGE/install.sh $STAGE"

say "done. Caddy issues the certificate on the first request to the name."
say "Then, from here:"
say "  FORMA_SMOKE_TOKEN=\$(ssh <host> \"sudo -u forma /opt/forma/forma-server api-key smoke\") \\"
say "    ./scripts/smoke-remote.py https://<domain>"
