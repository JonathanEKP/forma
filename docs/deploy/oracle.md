# Provisioning the VM for a remote Forma

What has to exist before the repo half is any use. Hand this to whoever is
building the machine; it is written to be followed without reading the rest of
the project.

Plan sections 6.1, 56, 57, 89 and 90 are what this implements. **§90 warns that
Oracle's Always Free capacity is regularly unavailable in a given region, and
that the architecture does not depend on it** — if the instance cannot be
created, any small ARM or x86 Linux box with a public address works the same
way, and only the "Instance" section below changes.

---

## 1. The instance

| | |
| --- | --- |
| Shape | Ampere A1 (`VM.Standard.A1.Flex`) |
| Architecture | **aarch64**, not x86 — §89 says so explicitly |
| Size | 2 OCPU, 12 GB RAM (inside Always Free: 4 OCPU / 24 GB total) |
| OS | Ubuntu 24.04 LTS **arm64** |
| Boot volume | 50 GB default is enough to start |

Try the instance first, before anything else on this list. Capacity is the item
most likely to fail and everything else is wasted if it does.

## 2. Network, and the trap

Two firewalls have to agree, and forgetting the second is the classic way an
Oracle box looks broken:

1. **The VCN security list** (or a Network Security Group): ingress on **80** and
   **443** from `0.0.0.0/0`.
2. **The instance's own `iptables`.** Oracle's Ubuntu images ship with a
   restrictive `INPUT` chain that drops 80 and 443 even when the cloud firewall
   allows them. Open them on the host too and persist the rules.

**Do not expose 4765 or 4766.** Both processes bind `127.0.0.1` and Caddy is the
only thing the internet talks to. If those ports are reachable from outside, the
bearer token is the only thing between the internet and the design database, and
that is not the design.

## 3. A domain name

The longest lead time on this list, so start it early.

**For now: `forma.hintly.online`.** Forma has no domain of its own yet, so this
is a subdomain borrowed from one that does exist — see the decision note in
`docs/plan-status.md`. A subdomain rather than the apex, so whatever
`hintly.online` already serves is untouched and the certificate covers this name
only.

Create:

```
forma.hintly.online.   A   <the instance's public IP>
```

Then, before Caddy first runs:

- **Port 80 has to be reachable.** Caddy's automatic HTTPS solves an ACME
  challenge there, and it will keep failing quietly until it can.
- **The name has to resolve.** `dig +short forma.hintly.online` from anywhere
  should return the instance's address.

An IP address alone will not do: Caddy issues a certificate for a name, not for
an address, and MCP hosts want an `https://` URL.

The name is `FORMA_DOMAIN` in `/etc/forma/forma.env`, not a literal in the
Caddyfile, precisely because it is temporary. Moving to the real domain later is
a new A record, one line in that file and a Caddy reload — no editing files on
the machine.

## 4. Users, directories, packages

```bash
sudo useradd --system --create-home --home-dir /var/lib/forma forma
sudo mkdir -p /opt/forma /etc/forma /var/lib/forma
sudo chown -R forma:forma /var/lib/forma
```

| Path | Holds |
| --- | --- |
| `/opt/forma` | the two binaries and `web/` (the built SPA) |
| `/etc/forma/forma.env` | configuration, including the token. `chmod 600`, owned by root |
| `/var/lib/forma` | `forma.db`, `assets/`, `previews/`, `exports/`, `fonts/` |

Packages: `caddy` (from the official repository, not the distro's old build) and
`ca-certificates`. Nothing else — the binaries are static enough that no Rust
toolchain is needed on the VM, and building there on 2 OCPU is slow anyway.

## 5. The token

Generate it on the VM and never paste it into a chat:

```bash
openssl rand -base64 48
```

It goes in `/etc/forma/forma.env` as `FORMA_REMOTE_TOKEN`. The server stores and
compares a **hash** of it (§57), so the file is the only place the value exists.
Losing it means generating a new one, not recovering it.

## 6. What the repo half will expect

So both halves meet. These names are the contract:

```
services   forma-server.service, forma-mcp.service
user       forma
binaries   /opt/forma/forma-server, /opt/forma/forma-mcp
web        /opt/forma/web            (served by forma-server)
config     /etc/forma/forma.env
data       /var/lib/forma
ports      127.0.0.1:4765  server
           127.0.0.1:4766  MCP over HTTP
caddy      /mcp  -> 4766      (§56)
           /     -> 4765
```

Unit files and a Caddyfile ship in `deploy/`; they are not written by hand on the
machine.

## 7. Backups

`docs/backup.md` is the procedure and it does not change here, but the schedule
does: on a laptop a backup is something you run, on a VM it is something that has
to happen without you. A timer calling `forma-server backup` into a directory on
a different volume, plus whatever pulls it off the machine.

Oracle's block-volume backups are not a substitute. `forma-server backup` is safe
to run hot because it uses `VACUUM INTO`; a volume snapshot of a live SQLite with
a write-ahead log is not.

## 8. Before you say it is ready

- [ ] `ssh` in as a non-root user with a key
- [ ] `curl -I http://<domain>` reaches the box from outside (before TLS exists)
- [ ] `uname -m` says `aarch64`
- [ ] `caddy version` runs
- [ ] `/etc/forma/forma.env` exists, `chmod 600`, with `FORMA_REMOTE_TOKEN` set
- [ ] `df -h /var/lib/forma` shows room to grow
- [ ] 4765 and 4766 are **not** reachable from outside

Then the repo half is: build for `aarch64-unknown-linux-gnu`, copy the two
binaries and `apps/web/dist`, install the units and the Caddyfile, start, and run
the external smoke tests.

## 9. What was checked before anything reached the machine

Not to be repeated on the VM, but worth knowing was done:

- The two binaries cross-compile and **link** for `aarch64-unknown-linux-gnu`
  (`scripts/build-aarch64.sh`), and run on `ubuntu:24.04` arm64 under emulation.
- Server, MCP-over-HTTP and the bearer all work there: 401 without a token, the
  MCP handshake with one, and a `tools/call` whose answer proves the token
  travels onward to the REST API.
- **The same page renders byte-identically on x86_64 and aarch64** — same
  SHA-256, same length. It is the promise the whole project rests on and the
  rasteriser takes different SIMD paths per architecture, so it was worth
  measuring rather than assuming. It also means the golden tests will pass on
  the VM.
