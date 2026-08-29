# Remote deploy: where this actually is

State, not intent. [oracle.md](oracle.md) says what the machine has to be; this
says what it is, what has been checked, and by whom.

Last updated 2026-08-29.

**Two kinds of claim below, kept apart on purpose.** *Reported* means it comes
from the provisioning handoff and nobody here re-checked it. *Verified here*
means it was run on this side and the output was read. Mixing the two is how a
status document starts lying.

---

## The short version

The machine exists and answers on port 80 through its temporary domain. The
build artefact exists and is proven to work on the target architecture. **They
have not met yet:** nothing from the repo has been copied to the VM.

| | |
| --- | --- |
| VM provisioned, reachable, Caddy running | reported ✅ |
| aarch64 build, and the stack running on it | verified here ✅ |
| Binaries, SPA, units, Caddyfile installed on the VM | not started ❌ |
| HTTPS | not yet — Caddy still serves its own default site |
| External smoke tests | not run |
| Backups on the VM | not configured |

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

> **Open item, and the one worth closing first.** The handoff says persistence
> across reboot was *not* explicitly verified. If those rules are not saved,
> the first reboot takes out HTTP and HTTPS together — and with them the
> certificate renewal, silently, until someone notices. `netfilter-persistent
> save`, then reboot, then re-check 80 and 443 from outside.

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

## What has not happened

Nothing from the repo is on the VM. In order:

1. Copy `dist/aarch64-unknown-linux-gnu/` into `/opt/forma` and `/var/lib/forma/fonts`.
2. Install both units, the Caddy drop-in, and Forma's Caddyfile.
3. Write `/etc/forma/domain.env`; add the missing keys to `/etc/forma/forma.env`
   from `forma.env.example` (the token is already there).
4. Start both services; reload Caddy and let it issue the certificate.
5. Smoke test from outside: HTTPS, the editor, and an MCP handshake against
   `https://forma.hintly.online/mcp`.
6. A backup timer calling `forma-server backup`, per [backup.md](../backup.md).
   On a laptop a backup is something you run; on a VM it has to happen without
   you, and an Oracle volume snapshot is not a substitute for `VACUUM INTO`.

## What will not work, and is not a bug

**The browser editor is not usable remotely.** The API requires a bearer token
and the SPA has nowhere to hold one, so `https://forma.hintly.online` will serve
the editor and its API calls will come back 401. §57 calls this "first remote
testing" and Phase 11's own acceptance is *an authorized remote MCP host can
create/render a piece* — the agent half, which is what this deploy delivers. A
token box in the editor is a separate decision, not an oversight.
