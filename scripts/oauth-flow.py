#!/usr/bin/env python3
"""The remote MCP OAuth flow, observed from outside (plan section 111.2, 116.4).

    FORMA_REMOTE_TOKEN=... ./scripts/oauth-flow.py https://forma.hintly.online

Why this is a script and not a test: every test in this repository runs against
a temporary SQLite and a locally built binary, and CI has no deployment, no
domain and no certificate. Calling this a CI test would produce one that is
skipped on every run, which is worse than a script somebody executes.
`scripts/smoke-remote.py` is the precedent and this sits beside it.

It observes what a standards-based MCP client would do, in the order it would do
it. Steps that need a person in a browser stop and say so rather than pretending
to be automated -- there is no honest way to approve a consent screen from here,
and §2.2 forbids driving external services with a browser driver anyway.

It follows the URL the deployment advertises rather than one composed here.
That is the point of the exercise -- a client only knows what the challenge told
it -- and it means this cannot be run against a local stack without the reverse
proxy in front, where `/mcp` and the advertised `https://` name do not exist.
The first two checks pass locally; the rest need the deployment.

Results belong in docs/deploy/status.md under *verified here*.
"""

import base64
import hashlib
import json
import os
import secrets
import sys
import urllib.error
import urllib.parse
import urllib.request

BASE = (sys.argv[1] if len(sys.argv) > 1 else "").rstrip("/")

if not BASE:
    print(__doc__.strip().splitlines()[2].strip(), file=sys.stderr)
    sys.exit(2)

passed, failed, manual = 0, 0, 0


def check(name, expected, actual):
    global passed, failed
    if expected == actual:
        print(f"  ok      {name:<52} {actual}")
        passed += 1
    else:
        print(f"  FAIL    {name:<52} got {actual!r}, wanted {expected!r}")
        failed += 1


def note(name, detail=""):
    """A step only a person can take. Counted separately: it is not a pass."""
    global manual
    print(f"  manual  {name:<52} {detail}")
    manual += 1


def fetch(url, *, method="GET", body=None, headers=None, timeout=20):
    """Returns (status, headers, text). A refusal is an answer, not an exception."""
    data = None
    head = dict(headers or {})
    if body is not None:
        if isinstance(body, dict) and head.get("content-type", "").startswith("application/json"):
            data = json.dumps(body).encode()
        elif isinstance(body, dict):
            data = urllib.parse.urlencode(body).encode()
            head.setdefault("content-type", "application/x-www-form-urlencoded")
        else:
            data = body

    request = urllib.request.Request(url, data=data, headers=head, method=method)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, dict(response.headers), response.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as error:
        return error.code, dict(error.headers), error.read().decode("utf-8", "replace")
    except Exception as error:  # noqa: BLE001 - a connection refusal is a result
        return 0, {}, str(error)


def as_json(text, default=None):
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return default


# --- 1 and 2: the challenge -------------------------------------------------

print("The 401 that starts a discovery")

status, headers, _ = fetch(
    f"{BASE}/mcp",
    method="POST",
    body={"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
    headers={"content-type": "application/json", "accept": "application/json, text/event-stream"},
)
check("an unauthenticated /mcp is refused", 401, status)

challenge = headers.get("WWW-Authenticate") or headers.get("www-authenticate") or ""
check(
    "and the refusal says where to authenticate",
    True,
    "resource_metadata=" in challenge,
)

# The URL the client is told to fetch is the one it must fetch -- not one this
# script composes, which would test our guess rather than the deployment.
advertised = ""
if "resource_metadata=" in challenge:
    advertised = challenge.split("resource_metadata=", 1)[1].strip().strip('"')
print(f"          advertised: {advertised or '(none)'}")

# --- 3 and 4: protected resource metadata -----------------------------------

print("\nProtected Resource Metadata")

status, _, text = fetch(advertised) if advertised else (0, {}, "")
check("the advertised document is public", 200, status)

metadata = as_json(text, {})
check("it names this deployment's /mcp", f"{BASE}/mcp", metadata.get("resource"))

servers = metadata.get("authorization_servers") or []
check("and names an authorization server", True, len(servers) == 1)
issuer = servers[0] if servers else ""
print(f"          issuer: {issuer or '(none)'}")

# --- 5: the authorization server's own metadata -----------------------------

print("\nThe authorization server")

status, _, text = fetch(f"{issuer}/.well-known/oauth-authorization-server") if issuer else (0, {}, "")
check("advertises its own metadata", 200, status)
server = as_json(text, {})

for field in ("authorization_endpoint", "token_endpoint", "registration_endpoint"):
    check(f"  {field}", True, bool(server.get(field)))

check(
    "supports PKCE with S256",
    True,
    "S256" in (server.get("code_challenge_methods_supported") or []),
)

# --- 6: dynamic client registration -----------------------------------------

print("\nRegistering a client, as an MCP host would")

registration = server.get("registration_endpoint")
redirect_uri = "http://127.0.0.1:9876/callback"
client_id = None

if registration:
    status, _, text = fetch(
        registration,
        method="POST",
        body={
            "client_name": "Forma flow check",
            "redirect_uris": [redirect_uri],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        },
        headers={"content-type": "application/json"},
    )
    check("registration is accepted", True, status in (200, 201))
    client_id = as_json(text, {}).get("client_id")
    check("and returns a client id", True, bool(client_id))
else:
    check("registration is accepted", True, False)

# --- 7 and 8: the part a person must do -------------------------------------

print("\nAuthorization")

if client_id:
    verifier = base64.urlsafe_b64encode(secrets.token_bytes(32)).decode().rstrip("=")
    challenge_value = (
        base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).decode().rstrip("=")
    )
    state = secrets.token_urlsafe(16)
    authorize = server.get("authorization_endpoint", "") + "?" + urllib.parse.urlencode(
        {
            "response_type": "code",
            "client_id": client_id,
            "redirect_uri": redirect_uri,
            "scope": "openid email",
            "state": state,
            "code_challenge": challenge_value,
            "code_challenge_method": "S256",
        }
    )

    note("open this and approve it", "")
    print(f"\n{authorize}\n")
    print("          It must land on Forma's own consent page, name the client,")
    print("          and return to the redirect above carrying ?code= and ?state=.")
    note("the consent page renders and names the client")
    note("approving returns the code and the matching state")

    print("\n  Paste the full redirected URL to continue, or press Enter to stop here.")
    try:
        redirected = input("  > ").strip()
    except EOFError:
        redirected = ""

    if redirected:
        query = urllib.parse.parse_qs(urllib.parse.urlparse(redirected).query)
        check("the state comes back unchanged", [state], query.get("state"))
        code = (query.get("code") or [""])[0]
        check("an authorization code came back", True, bool(code))

        # --- 9: the exchange ------------------------------------------------
        print("\nThe token, and what it opens")
        status, _, text = fetch(
            server.get("token_endpoint", ""),
            method="POST",
            body={
                "grant_type": "authorization_code",
                "code": code,
                "redirect_uri": redirect_uri,
                "client_id": client_id,
                "code_verifier": verifier,
            },
        )
        check("the code exchanges for a token", 200, status)
        tokens = as_json(text, {})
        access, refresh = tokens.get("access_token"), tokens.get("refresh_token")

        if access:
            claims = json.loads(
                base64.urlsafe_b64decode(access.split(".")[1] + "==").decode()
            )
            # The audience is what stops this being spendable as a browser
            # session, and it must name the client that holds it.
            print(f"          aud:       {claims.get('aud')}")
            print(f"          client_id: {claims.get('client_id')}")
            check(
                "the audience names this deployment's MCP resource",
                True,
                str(claims.get("aud", "")).startswith(f"{BASE}/mcp"),
            )
            check(
                "and names the client holding it",
                client_id,
                claims.get("client_id"),
            )

            # --- 10 and 11 --------------------------------------------------
            status, _, text = fetch(
                f"{BASE}/mcp",
                method="POST",
                body={
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2025-06-18",
                        "capabilities": {},
                        "clientInfo": {"name": "flow-check", "version": "0"},
                    },
                },
                headers={
                    "content-type": "application/json",
                    "accept": "application/json, text/event-stream",
                    "authorization": f"Bearer {access}",
                },
            )
            check("the token opens /mcp", True, status == 200)

            # --- 12 -------------------------------------------------------
            if refresh:
                status, _, text = fetch(
                    server.get("token_endpoint", ""),
                    method="POST",
                    body={
                        "grant_type": "refresh_token",
                        "refresh_token": refresh,
                        "client_id": client_id,
                    },
                )
                check("refreshing returns a new token", 200, status)

            note("revoke this connection from Forma's Agents page")
            note("then confirm it can no longer refresh or reconnect")
    else:
        note("the exchange, the tool call, refresh and revocation")
else:
    note("everything past registration")

print(f"\n{passed} passed, {failed} failed, {manual} for a person")
sys.exit(1 if failed else 0)
