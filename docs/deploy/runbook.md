# Deploying Forma to the VM

The continuation of [oracle.md](oracle.md), which is about building the machine.
This is about putting Forma on it. [status.md](status.md) says where things
currently stand.

Written to be followed by someone who has not read the rest of the project.

---

## Who does what

The build needs the repository and a toolchain; the install needs `sudo` on the
VM. Those are usually two different people or two different machines, so the
split is stated rather than assumed.

| Step | Where | Needs |
| --- | --- | --- |
| 0. Close the reboot question | on the VM | `sudo` |
| 1. Build and install | a machine with the repo, over `ssh` | repo, Rust, pnpm, a cross toolchain |
| 2. Verify | anywhere with network | the token |
| 3. Backups | on the VM | `sudo` |

If whoever holds the VM also has the repo and a toolchain, they can do all of it.
If not, step 1 belongs to whoever has the repo.

---

## 0. Before deploying: does the firewall survive a reboot?

**Do this first.** The host's `iptables` rules for 80 and 443 were added by hand
during provisioning and persistence was never confirmed. If they are not saved,
the first reboot takes out HTTP, HTTPS and Caddy's certificate renewal together,
and quietly — nobody notices until a certificate expires.

```bash
sudo apt install -y iptables-persistent   # or: netfilter-persistent
sudo netfilter-persistent save
sudo reboot
```

Then, from anywhere:

```bash
curl -I http://forma.hintly.online
```

It has to answer before you go on. Do **not** run `iptables -F` at any point:
Oracle's rules for `169.254.0.0/16` and `InstanceServices` live in that chain,
and flushing them breaks instance metadata.

---

## 1. Build and install

From the repo root, on a machine that can reach the VM over `ssh`:

```bash
./scripts/deploy.sh ubuntu@129.213.28.225 -i ~/.ssh/id_rsa2
```

That is the whole command. It builds for `aarch64-unknown-linux-gnu`, stages
everything into `/tmp/forma-deploy` on the VM, and runs `deploy/install.sh` there
under `sudo`.

**What it needs on the build machine:** Rust with the `aarch64-unknown-linux-gnu`
target (the script adds it), `pnpm` for the editor, and a cross toolchain — either
`gcc-aarch64-linux-gnu`, which needs root, or `zig`, which does not. Two
dependencies compile C, so a rustup target alone is not enough.

**What `install.sh` will not do**, because these are the ways a deploy script
ruins someone's day:

- It never writes the token and never overwrites `/etc/forma/forma.env`. Keys
  that are missing get appended; everything else is left alone. It refuses to run
  if that file is absent or has no `FORMA_REMOTE_TOKEN`.
- It checks the staged binary really is `aarch64` before touching anything.
- It validates the Caddyfile before reloading, since one that fails to parse
  takes the site down rather than leaving the old one running.
- It keeps the build it replaces in `/opt/forma/previous`.

It stops both services before replacing the binaries. That is not politeness: a
running executable cannot be overwritten in place, and the kernel answers
`ETXTBSY`.

It is idempotent. Run it again for every deploy.

### If it fails

```bash
sudo systemctl status forma-server forma-mcp caddy
sudo journalctl -u forma-server -n 50 --no-pager
```

The three failures worth naming in advance:

- **`forma-server` exits immediately** with a message about `FORMA_REMOTE_TOKEN`.
  It is refusing to bind a non-loopback address without a credential. Either the
  token is missing from `forma.env` or `FORMA_BIND` is not `127.0.0.1:4765`.
- **`forma-mcp` exits immediately.** Same credential, different message: HTTP
  mode has no unauthenticated mode at all.
- **Caddy serves but the certificate never appears.** Port 80 is not reachable
  from outside, or the name does not resolve here yet. `journalctl -u caddy` says
  which. Check step 0.
- **Caddy validates and then fails to start**, usually `permission denied` on
  something it writes. Validation provisions the config's modules, so anything it
  opens is created by whoever ran it; the installer now validates as the `caddy`
  user precisely so a permission the runtime lacks is one validation lacks too.
  If you add a `log { output file ... }` to the Caddyfile, create the file owned
  by `caddy` first — Caddy makes log files `0600` owned by the opener.

- **`/mcp` answers 403 `Host header is not allowed`**, and every MCP check fails
  together because the refusal happens before any JSON-RPC is read. rmcp
  validates the inbound Host against an allowlist that defaults to loopback, and
  behind Caddy the Host is the public name. `FORMA_DOMAIN` is what names the
  deployment; `forma-mcp.service` reads `/etc/forma/domain.env` for it, so a
  missing or stale value there is the thing to check. `journalctl -u forma-mcp`
  prints the accepted hosts at startup.

A deploy from before 2026-08-29 may have left `/var/log/caddy/forma.log` owned by
root. Nothing references it any more; it can be deleted or ignored.

### Rolling back

```bash
sudo bash /tmp/forma-deploy/install.sh --rollback
```

Restores the previous binaries and editor and restarts. The data and the config
are never touched by either direction, so a rollback cannot lose work.

---

## 2. Verify, from off the machine

```bash
export FORMA_REMOTE_TOKEN=$(ssh -i ~/.ssh/id_rsa2 ubuntu@129.213.28.225 \
    "sudo sed -n 's/^FORMA_REMOTE_TOKEN=//p' /etc/forma/forma.env")

./scripts/smoke-remote.py https://forma.hintly.online
```

Fifteen checks, and it must end `0 failed`:

- TLS, and that the origin requires the credential
- the credential refused, wrong, and accepted on both the API and `/mcp`
- **4765 and 4766 are not reachable from outside** — the negative check, which is
  the one nobody writes and everybody needs
- the MCP handshake, and a `tools/call` whose answer proves the token reaches the
  REST API behind it
- Phase 11's acceptance: an authorized host creates a design and renders it

It deletes the project it made. If it fails partway it still tries to.

The command reads the token over `ssh` into an environment variable rather than
printing it. Do not paste the value anywhere.

---

## 3. Backups

Installed by the deploy — `forma-backup.timer` is enabled with everything else,
so there is nothing to do by hand. What follows is what it does and how to get
your data back, because a backup nobody has restored is a hope rather than a
backup.

### What runs

```
forma-backup.timer     daily, Persistent=true, up to 15 minutes of jitter
  └── forma-backup.service   oneshot, as the forma user
        └── /opt/forma/backup.sh
              ├── forma-server backup      → /var/lib/forma/backups/<UTC>
              └── retention
```

**The destination is not passed, on purpose.** `forma-server backup` with no
argument writes to `DATA_DIR/backups/<UTC timestamp>`, unique per run by
construction. An earlier draft of this document had systemd compute the path with
`%i` — an instance specifier, which means nothing in a unit that is not a
template. Every run would have targeted the same directory, and the command
refuses a non-empty destination, so every run after the first would have failed.
It was caught in review rather than in production; it is written down because the
next person to reach for `%i` should meet this first.

`Persistent=true` means a machine that was off at the appointed time takes its
backup when it comes back, instead of skipping the day quietly.

### Retention, and how a half-written backup is told apart

`manifest.json` is written **last**, after the database and the assets. Its
presence is therefore the difference between a backup and an interrupted attempt,
and the script uses exactly that: a directory without one is removed rather than
counted. Counting it would let retention discard a real backup to keep a broken
one.

Fourteen are kept, oldest pruned first; `FORMA_BACKUP_KEEP` in
`/etc/forma/forma.env` changes that.

Retention only runs when the backup succeeded — the script exits first otherwise,
so a failed run leaves its partial directory on disk for whoever reads
`journalctl -u forma-backup`, and the next successful run clears it.

### Checking it works

```bash
sudo systemctl start forma-backup.service     # don't wait for midnight
systemctl status forma-backup.service
sudo ls -la /var/lib/forma/backups
systemctl list-timers forma-backup.timer
```

A finished backup holds `forma.db`, `assets/`, and `manifest.json`. Read the
manifest: it records how many projects, designs, versions and assets it contains,
which is the cheapest way to notice a backup of an empty database.

### Restoring

[backup.md](../backup.md) is the procedure and it does not change here; only the
paths and the service names do.

```bash
sudo systemctl stop forma-mcp forma-server
sudo mv /var/lib/forma/forma.db /var/lib/forma/forma.db.aside   # aside, not away
sudo -u forma cp /var/lib/forma/backups/<stamp>/forma.db /var/lib/forma/forma.db
sudo -u forma cp -r /var/lib/forma/backups/<stamp>/assets/. /var/lib/forma/assets/
sudo systemctl start forma-server forma-mcp
```

Move the current database aside rather than deleting it: if the backup turns out
to be the wrong one, you still have what you had. The fonts are not in a backup —
they arrive with the deploy — and `previews/` and `exports/` are excluded on
purpose, since both regenerate.

There is deliberately no restore command. It would overwrite the user's current
work on a mistyped argument, which is exactly the operation that should be slow
and deliberate.

### Still open: getting them off the machine

**A backup on the same volume as the database survives a mistake, not a lost
instance.** Nothing here addresses that yet, and no destination has been chosen.
Whatever it turns out to be, it should pull rather than be pushed to — a machine
that can delete its own off-site copies has not got off-site copies — and it
should copy only finished backups, which `manifest.json` identifies.

## What will not work, and is not a bug

**The browser editor is not usable remotely.** Every path except
`/api/v1/health` requires the bearer, the editor included, so the domain answers
401 to a visitor rather than serving an app whose every call then fails in the
console. The SPA has nowhere to hold a token.

Plan §57 calls this "first remote testing", and Phase 11's acceptance is *an
authorized remote MCP host can create/render a piece* — the agent half, which is
what this delivers. A token box in the editor is a separate decision.

## Pointing an MCP host at it

Once the smoke test passes, the endpoint is:

```
https://forma.hintly.online/mcp
Authorization: Bearer <the token>
```

Remember that the name is temporary — `hintly.online` belongs to another project.
Whatever is configured against it has to be repointed when Forma has its own.
