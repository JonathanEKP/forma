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

Not optional once this is a machine rather than a laptop. On a laptop a backup is
something you run; here it has to happen without you.

```bash
sudo tee /etc/systemd/system/forma-backup.service >/dev/null <<'UNIT'
[Unit]
Description=Forma backup
[Service]
Type=oneshot
User=forma
EnvironmentFile=/etc/forma/forma.env
ExecStart=/opt/forma/forma-server backup /var/lib/forma/backups/%%i
UNIT

sudo tee /etc/systemd/system/forma-backup.timer >/dev/null <<'UNIT'
[Unit]
Description=Forma backup, daily
[Timer]
OnCalendar=daily
Persistent=true
[Install]
WantedBy=timers.target
UNIT

sudo systemctl enable --now forma-backup.timer
```

Read [backup.md](../backup.md) before relying on it; in particular, `forma-server
backup` refuses a non-empty destination, so the destination has to vary — a date
in the path. And an Oracle block-volume snapshot is **not** a substitute: it
copies a live SQLite with a write-ahead log, which `VACUUM INTO` is specifically
there to avoid.

Getting the backups off the machine is a separate question and still open.

---

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
