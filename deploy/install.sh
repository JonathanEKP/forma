#!/usr/bin/env bash
# Installs a staged Forma build on the VM. Run as root, on the machine.
#
# Idempotent: run it again for every deploy. It never writes the token and never
# overwrites /etc/forma/forma.env -- that file holds the credential, and a deploy
# script that can destroy it is a deploy script that eventually will. Missing
# keys are appended; existing ones are left exactly as they are.
set -euo pipefail

STAGE=${1:-/tmp/forma-deploy}
say() { printf '==> %s\n' "$*"; }

[ "$(id -u)" -eq 0 ] || { echo "run as root" >&2; exit 1; }
[ -x "$STAGE/forma-server" ] || { echo "no build staged in $STAGE" >&2; exit 1; }

if ! file "$STAGE/forma-server" | grep -q aarch64; then
    echo "$STAGE/forma-server is not an aarch64 binary. Wrong build (plan §89)." >&2
    exit 1
fi

# --- config ---------------------------------------------------------------
install -d -o root -g root -m 755 /etc/forma
if [ ! -f /etc/forma/forma.env ]; then
    echo "/etc/forma/forma.env does not exist." >&2
    echo "Create it with FORMA_REMOTE_TOKEN first; see docs/deploy/oracle.md §5." >&2
    exit 1
fi
if ! grep -q '^FORMA_REMOTE_TOKEN=' /etc/forma/forma.env; then
    echo "/etc/forma/forma.env has no FORMA_REMOTE_TOKEN." >&2
    echo "Without it forma-server refuses a public bind and forma-mcp refuses HTTP." >&2
    exit 1
fi
chown root:root /etc/forma/forma.env
chmod 600 /etc/forma/forma.env

say "filling in any missing keys in /etc/forma/forma.env"
while IFS= read -r line; do
    case "$line" in
        ''|\#*) continue ;;
    esac
    key=${line%%=*}
    grep -q "^$key=" /etc/forma/forma.env || {
        printf '%s\n' "$line" >> /etc/forma/forma.env
        echo "    added $key"
    }
done < "$STAGE/forma.env.example"

if [ ! -f /etc/forma/domain.env ]; then
    say "creating /etc/forma/domain.env"
    install -o root -g root -m 644 "$STAGE/domain.env.example" /etc/forma/domain.env
fi

# --- stop, so the binaries are replaceable --------------------------------
# A running executable cannot be overwritten in place: the kernel answers
# ETXTBSY. Stopping is not politeness, it is the only way this works.
say "stopping services"
systemctl stop forma-mcp.service 2>/dev/null || true
systemctl stop forma-server.service 2>/dev/null || true

# --- binaries, editor, fonts ----------------------------------------------
say "installing binaries and the editor into /opt/forma"
install -d -o root -g root -m 755 /opt/forma
install -o root -g root -m 755 "$STAGE/forma-server" "$STAGE/forma-mcp" /opt/forma/
rm -rf /opt/forma/web
cp -r "$STAGE/web" /opt/forma/web
chown -R root:root /opt/forma/web

# The renderer loads only from FORMA_FONT_DIR and never from fonts installed on
# the host. That is what makes this machine render what the build machine
# renders, so the fonts ship with the build rather than being apt-installed.
say "installing fonts into /var/lib/forma/fonts"
install -d -o forma -g forma -m 750 /var/lib/forma
rm -rf /var/lib/forma/fonts
cp -r "$STAGE/fonts" /var/lib/forma/fonts
chown -R forma:forma /var/lib/forma/fonts

# --- units ----------------------------------------------------------------
say "installing systemd units"
install -o root -g root -m 644 "$STAGE/systemd/forma-server.service" \
    "$STAGE/systemd/forma-mcp.service" /etc/systemd/system/
install -d -o root -g root -m 755 /etc/systemd/system/caddy.service.d
install -o root -g root -m 644 "$STAGE/systemd/caddy.service.d/forma-domain.conf" \
    /etc/systemd/system/caddy.service.d/
systemctl daemon-reload

# --- caddy ----------------------------------------------------------------
say "installing the Caddyfile"
install -o root -g root -m 644 "$STAGE/caddy/Caddyfile" /etc/caddy/Caddyfile
# Validate before reloading. A Caddyfile that fails to parse takes the site down
# rather than leaving the old one running.
# shellcheck disable=SC1091
set -a; . /etc/forma/domain.env; set +a
caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null

# --- start ----------------------------------------------------------------
say "starting"
systemctl enable --now forma-server.service
systemctl enable --now forma-mcp.service
systemctl restart caddy.service

sleep 2
for unit in forma-server forma-mcp caddy; do
    if systemctl is-active --quiet "$unit"; then
        echo "    $unit active"
    else
        echo "    $unit FAILED" >&2
        journalctl -u "$unit" -n 20 --no-pager >&2
        exit 1
    fi
done

say "installed. Caddy issues the certificate on first request to the domain;"
say "give it a few seconds, then run the external smoke tests."
