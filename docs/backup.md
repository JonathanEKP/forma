# Backing up and restoring

Everything Forma holds lives in one directory, `FORMA_DATA_DIR` (`./data` by
default). Two things in it are irreplaceable and the rest is not.

## Taking a backup

```bash
./target/release/forma-server backup
```

That writes to `DATA_DIR/backups/<utc timestamp>` and prints the path. Give it a
directory to put it somewhere else:

```bash
./target/release/forma-server backup /mnt/backups/forma-2026-08-28
```

It works while the server is running, and it refuses to write into a directory
that already holds something — a backup merged with an older one would restore
as a mixture of the two.

Scheduling it is a cron line; Forma has no scheduler of its own and does not
want one.

## What is in it

| | |
| --- | --- |
| `forma.db` | every project, Brand Kit, design and version snapshot, plus asset and export records |
| `assets/` | the image bytes people uploaded |
| `manifest.json` | when it was taken, by which version of Forma, and what it counted |

The database is copied with SQLite's `VACUUM INTO` rather than by copying the
file. A running server keeps recent writes in a write-ahead log, so copying
`forma.db` on its own can miss them, and copying the three files separately is
not atomic. `VACUUM INTO` asks SQLite for a consistent snapshot in a single
file.

**Deliberately left out**, because both regenerate from what is kept:

- `previews/` — a render cache, rebuilt on demand.
- `exports/` — each export pins the version it came from, and that version is in
  the database, so it can be exported again.

One limitation of copying a running server: an upload that lands during the copy
may be missed. Its row would be in the database with no file behind it, and
re-uploading the image fixes it. Take the backup with the server stopped if you
want to rule that out.

## Restoring

There is no restore command on purpose. Restoring overwrites the work that is
there now, and that should be a decision someone makes deliberately rather than
a flag someone passes by accident.

Stop the server first. It folds the write-ahead log back into the database as it
stops, so a stopped Forma is a consistent one.

```bash
# 1. stop the server, then move the current data aside rather than deleting it
mv data data-before-restore

# 2. put the backup in its place
mkdir data
cp -r /path/to/backup/forma.db /path/to/backup/assets data/

# 3. fonts are not in the backup: they come with the repository
cp -r data-before-restore/fonts data/

# 4. start it
./target/release/forma-server
```

Then open the editor and check a design opens and renders. The first render will
be slower than usual because the preview cache starts empty, which is expected.

If it looks right, `data-before-restore` can go.

## Checking a backup is real

A backup nobody has restored is a guess. The quickest check without touching
your live data is to run a second Forma against a copy:

```bash
cp -r /path/to/backup /tmp/forma-restore-check
FORMA_DATA_DIR=/tmp/forma-restore-check \
FORMA_DATABASE_URL=sqlite:///tmp/forma-restore-check/forma.db \
FORMA_BIND=127.0.0.1:4790 \
  ./target/release/forma-server
```

Open <http://127.0.0.1:4790>, and confirm the projects are listed, a design
opens, and it renders — rendering is the real check, because it needs both the
database and the asset bytes.
