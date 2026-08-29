# Remote deploy: where this actually is

State, not intent. [oracle.md](oracle.md) says what the machine has to be,
[runbook.md](runbook.md) how to deploy onto it; this says what it is, what has
been checked, and by whom.

Last updated 2026-08-29.

**Two kinds of claim below, kept apart on purpose.** *Reported* means it comes
from the provisioning handoff and nobody here re-checked it. *Verified here*
means it was run on this side and the output was read. Mixing the two is how a
status document starts lying.

---

## The short version

**Deployed, and Phase 11's acceptance is met.** `scripts/smoke-remote.py` against
`https://forma.hintly.online` reports 15 passed, 0 failed — including the one
that is the phase's own criterion: an authorized remote MCP host creating a
design and rendering it.

| | |
| --- | --- |
| VM provisioned, reachable, Caddy running | reported ✅ |
| aarch64 build, and the stack running on it | verified here ✅ |
| Binaries, SPA, units, Caddyfile installed | reported ✅ |
| HTTPS, and the whole origin behind the credential | smoke ✅ |
| 4765 and 4766 unreachable from outside | smoke ✅ |
| Phase 11 acceptance: create and render, remotely | smoke ✅ |
| Backups on the VM | not configured ❌ |
| Host firewall survives a reboot | **still unconfirmed** ⚠ |

The MCP endpoint is `https://forma.hintly.online/mcp`, bearer-authenticated. The
hostname is temporary; see the decision note in `plan-status.md`.

---

## The machine, as reported

Oracle Cloud, US East (Ashburn), **AD-1**. AD-3 was tried first and answered
`Out of capacity for shape VM.Standard.A1.Flex`, which is §90's warning arriving
exactly as written — worth recording, because the next person to create an
instance will meet it too.

```
instance   forma            VM.Standard.A1.Flex, on-demand
cpu/ram    2 OCPU, 12 GB    aarch64 confirmed by uname -m
os         Ubuntu 24.04.4 LTS (Noble Numbat)
disk       ~46.6 GB boot volume, ~42 GB free, no extra block volume
network    VCN forma-vcn 10.0.0.0/16, public subnet 10.0.0.0/24, IPv6 off
access     ssh -i ~/.ssh/id_rsa2 ubuntu@<public ip>
```

**The public IP is operational state, not architecture.** It is currently
`129.213.28.225`. Nothing in the repo, the config or the units refers to it; the
name does. Treat a changed IP as a DNS edit, not a code change.

### Firewalls

Both layers are open on 80 and 443, and **4765 and 4766 are not exposed** —
which is the design, not an omission: both processes bind loopback and Caddy is
the only thing the internet talks to.

The host's own `iptables` did ship restrictive, exactly as `oracle.md` warned,
and rules were added before the final REJECT. Oracle's `169.254.0.0/16` and
`InstanceServices` rules were left alone; `iptables -F` would remove them and
break instance metadata.

> **Still open, and now the largest single risk to a working deployment.**
> Persistence across reboot was never confirmed. The deploy succeeded, so the
> rules are in place *now*; nothing says they come back. If they do not, the
> first reboot takes out HTTP and HTTPS together — and with them Caddy's
> certificate renewal, silently, until a certificate expires weeks later.
>
> `netfilter-persistent save`, reboot, then `curl -I https://forma.hintly.online`
> from outside. It is ten minutes and it is the difference between a deployment
> that survives a restart and one that appears to.

### Users and directories, as reported

```
forma          uid 999, gid 988
/opt/forma     root:root      binaries and web/
/etc/forma     root:root      configuration and secrets
/var/lib/forma forma:forma    750, the data
```

This matches the contract in `oracle.md` §6 and the units in `deploy/`.

### Caddy

v2.11.4 from the official repository, `active (running)`, **still serving its own
default site**. Forma's Caddyfile is not installed, so `/mcp → 4766` and
`/ → 4765` are not yet in effect.

### The token

Generated on the VM, never pasted anywhere. `/etc/forma/forma.env` exists,
`600 root:root`, with `FORMA_REMOTE_TOKEN` present — confirmed by testing for the
key without printing the value. **The value is not in this repo and must not be.**

### The name

`forma.hintly.online` → `129.213.28.225`, A record at Namecheap, resolving
externally, and `curl -I` returns `HTTP/1.1 200 OK` with `Server: Caddy`. So the
whole path works today: DNS → OCI security list → host iptables → Caddy on 80.

**Temporary, and it has to stay temporary.** `hintly.online` belongs to another
project. Nothing internal depends on the hostname: it lives in
`/etc/forma/domain.env` and reaches Caddy through the systemd drop-in. Moving is
a new A record, one line, a Caddy restart, and repointing whatever MCP clients
were configured against it.

---

## The repo half, verified here

All of this was run on this machine and the output read.

**The build.** `scripts/build-aarch64.sh` produces
`dist/aarch64-unknown-linux-gnu/` at 23 MB: `forma-server` (14 MB), `forma-mcp`
(7.1 MB), `web/` and `fonts/`. Two dependencies compile C — `libsqlite3-sys` and
`zstd-sys` — so a rustup target is not enough on its own; the script uses
`gcc-aarch64-linux-gnu` when present and `zig` otherwise, which needs no root.

**It runs.** Both binaries start on `ubuntu:24.04` arm64 under emulation, and the
whole stack works there: 401 without a token, the MCP handshake with one, and a
`tools/call` whose answer proves the token travels onward to the REST API.

**It renders the same.** The same page rendered through the REST API on x86_64
and on aarch64 came back byte-identical — SHA-256 `abcbb375…e631f`, 20532 bytes
both times. The rasteriser takes different SIMD paths per architecture, so this
was the real question behind deploying to an Ampere box. It also means the golden
tests will pass there unchanged.

**What ships in `deploy/`:**

```
caddy/Caddyfile                    {$FORMA_DOMAIN:forma.hintly.online}
systemd/forma-server.service       SIGTERM, 45s to checkpoint the WAL
systemd/forma-mcp.service          --transport http, needs forma-server
systemd/caddy.service.d/forma-domain.conf
forma.env.example                  the token and paths
domain.env.example                 the public name, no secrets
```

The Caddy drop-in exists because of a bug found while checking the handoff
against the repo: the Caddyfile told the reader to set `FORMA_DOMAIN` in
`forma.env`, and Caddy never reads that file, so the default would have been used
silently. Giving Caddy that file instead would have put the token into the
environment of a process that only proxies, so the name has a file of its own.

---

## Fixed after the first deploy attempt

The first run reached Caddy validation and then Caddy failed to start:
`open /var/log/caddy/forma.log: permission denied`. The Caddyfile wrote an access
log to a file; `caddy validate` ran as root and created it `0600 root:root`; the
service runs as `caddy` and could not reopen its own log. The deploy passed and
the site was down.

Two changes, and the first is the one that matters: **the file log is gone.**
systemd already captures Caddy's stderr into the journal, which rotates and is
where anyone would look on this box, so the file bought nothing and cost a
startup failure. Access logging is still on, to stderr.

Second, the installer now runs `caddy validate` as the `caddy` user rather than
as root, so a permission the runtime lacks is a permission validation lacks too.
That generalises past this one file, which is why it is worth doing even with the
file log removed.

Neither was reproduced here — there is no Caddy on the build machine — so this
rests on the reported symptom and on how Caddy creates log files. The next deploy
is the test.

## Fixed after the second attempt

Caddy then reached `forma-mcp` and every MCP check failed together with
`403 Forbidden: Host header is not allowed`. They were all cascading from one
refusal that happens before any JSON-RPC is read.

rmcp validates the inbound `Host` against an allowlist defaulting to loopback —
real protection, not a nuisance: it stops a page in someone's browser being
pointed at an MCP server bound on their own machine. Behind Caddy the Host is
`forma.hintly.online`, which the server had never been told about.

`forma-mcp` now adds `FORMA_DOMAIN` to rmcp's loopback entries, and its unit
reads `/etc/forma/domain.env` so the name has one source of truth shared with
Caddy rather than being written in two files. The loopback entries stay, and
`disable_allowed_hosts()` is not used — an empty allowlist means "allow
everything", which would trade the protection for a shorter config. The accepted
hosts are logged at startup, so the next 403 of this kind is one `journalctl`
away.

Reproduced and tested here, unlike the Caddy fix: a test starts the real binary
without `FORMA_DOMAIN` and asserts 403 for a public Host, then with it and
asserts the handshake, and a second test asserts loopback still works.

## What is still open

**[runbook.md](runbook.md)** covers all of it: redeploying (idempotent, one
command), rolling back, and the backup timer.

1. **The reboot question above.** First, and it is not a formality.
2. **Backups.** `forma-server backup` on a timer, per [backup.md](../backup.md).
   Right now a deployment holding real work has no copy of it. An Oracle
   block-volume snapshot is not a substitute: it copies a live SQLite with a
   write-ahead log, which `VACUUM INTO` exists to avoid.
3. **Getting the backups off the machine.** A backup on the same volume as the
   database survives a mistake, not a lost instance.
4. **The domain.** `forma.hintly.online` is borrowed. Migrating is a new A
   record, `FORMA_DOMAIN` in `/etc/forma/domain.env` — which both Caddy and
   `forma-mcp` read — a restart, and repointing whatever MCP clients exist.

## What will not work, and is not a bug

**The browser editor is not usable remotely, and the whole origin says so.**
Every path except `/api/v1/health` requires the bearer, the editor included, so
`https://forma.hintly.online` answers 401 to a visitor rather than serving an app
whose every call then fails in the console. The SPA has nowhere to hold a token,
so serving it anonymously would buy nothing and mislead.

§57 calls this "first remote testing" and Phase 11's own acceptance is *an
authorized remote MCP host can create/render a piece* — the agent half, which is
what this deploy delivers. A token box in the editor is a separate decision, not
an oversight.
