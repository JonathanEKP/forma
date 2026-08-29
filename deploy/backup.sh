#!/usr/bin/env bash
# One backup, then retention. Run by forma-backup.service, as the forma user.
#
# The destination is deliberately not passed: `forma-server backup` with no
# argument writes to DATA_DIR/backups/<UTC timestamp>, which is unique per run by
# construction. An earlier draft of this had systemd compute the directory with
# %i, which is an instance specifier and means nothing in a unit that is not a
# template -- every run would have targeted the same path, and the command
# refuses a non-empty destination, so every run after the first would have failed.
set -euo pipefail

DATA_DIR=${FORMA_DATA_DIR:-/var/lib/forma}
BACKUPS="$DATA_DIR/backups"
KEEP=${FORMA_BACKUP_KEEP:-14}
# Overridable so this script can be run against a scratch directory rather than
# only in place on the VM. A retention script nobody can rehearse is one nobody
# should trust with `rm -rf`.
FORMA_BIN=${FORMA_BIN:-/opt/forma/forma-server}

"$FORMA_BIN" backup

# Everything past here is retention, and it only runs when the backup above
# succeeded: `set -e` exits first otherwise, which leaves a failed run's partial
# directory on disk for whoever reads the journal entry. The next successful run
# clears it.

[ -d "$BACKUPS" ] || exit 0

# A directory without manifest.json did not finish. The manifest is written last,
# after the database and the assets, so its presence is what separates a backup
# from an interrupted attempt -- and an interrupted attempt must never be counted
# as one, or retention quietly discards a real backup to keep a broken one.
while IFS= read -r -d '' candidate; do
    if [ ! -e "$candidate/manifest.json" ]; then
        echo "removing incomplete backup $(basename "$candidate")"
        rm -rf -- "$candidate"
    fi
done < <(find "$BACKUPS" -mindepth 1 -maxdepth 1 -type d -print0)

# Keep the newest $KEEP. The names are UTC timestamps in a sortable format, so
# lexical order is chronological order.
mapfile -t complete < <(find "$BACKUPS" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' | sort)
surplus=$(( ${#complete[@]} - KEEP ))
if [ "$surplus" -gt 0 ]; then
    for name in "${complete[@]:0:$surplus}"; do
        echo "pruning $name"
        rm -rf -- "${BACKUPS:?}/$name"
    done
fi

echo "$(( ${#complete[@]} - (surplus > 0 ? surplus : 0) )) backup(s) kept in $BACKUPS"
