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

# Where the first half leaves what the second half needs.
#
# The flow has a person in the middle of it, and that step can take minutes: a
# script that must stay alive across it is one lost to a closed terminal, a
# timeout, or a pipe. Worse, the PKCE verifier is generated before the browser
# step and is useless afterwards if the process that made it is gone -- so the
# authorization URL dies with it, and the only symptom is a code that cannot be
# exchanged.
# Beside the repository rather than in a temporary directory. The gap between
# the two halves is however long a person takes in a browser, and /tmp is swept
# on a schedule nobody here controls -- losing the verifier there costs another
# round trip through a consent screen to find out. Gitignored; override with
# FORMA_OAUTH_STATE.
STATE_FILE = os.environ.get(
    "FORMA_OAUTH_STATE",
    os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".oauth-flow.json"),
)
RESUME = sys.argv[2] if len(sys.argv) > 2 else ""

if not BASE:
    print(__doc__.strip().splitlines()[2].strip(), file=sys.stderr)
    print(
        "  and afterwards:  ./scripts/oauth-flow.py <base> '<the redirected URL>'",
        file=sys.stderr,
    )
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


def header(headers, name):
    """Case-insensitive lookup.

    HTTP header names are case-insensitive and every layer here disagrees about
    which case to use: Caddy sends `www-authenticate`, urllib normalises it to
    `Www-Authenticate`, and a plain dict remembers whichever it was given. This
    script spelled two of the three and reported the deployment broken when the
    fault was its own — a failure in the pessimistic direction, which is less
    dangerous than the other one and still a lie.
    """
    wanted = name.lower()
    return next((value for key, value in headers.items() if key.lower() == wanted), "")


def initialize(base, credential):
    """One `initialize`, so two callers cannot send subtly different ones.

    They did: the second copy omitted protocolVersion, capabilities and
    clientInfo, and rmcp answered 422 "Unexpected message, expect initialize
    request". The script reported the refreshed token as refused when the
    deployment had refused the *request* — a check that duplicated its subject
    instead of sharing it.
    """
    return fetch(
        f"{base}/mcp",
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
            "authorization": f"Bearer {credential}",
        },
    )


def as_json(text, default=None):
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return default


def save(state):
    with open(STATE_FILE, "w", encoding="utf-8") as handle:
        json.dump(state, handle)
    os.chmod(STATE_FILE, 0o600)
    print(f"          saved to {STATE_FILE}")


def load():
    try:
        with open(STATE_FILE, encoding="utf-8") as handle:
            return json.load(handle)
    except (OSError, json.JSONDecodeError):
        return None


def exchange(redirected):
    """The half that runs after a person has approved the request."""
    global passed, failed
    saved = load()
    if not saved:
        print(f"  no saved authorization in {STATE_FILE}; run the first half again")
        sys.exit(2)

    query = urllib.parse.parse_qs(urllib.parse.urlparse(redirected).query)
    print("Coming back from the browser")
    # The state is what ties this redirect to the request we started, and
    # checking it is the whole defence against being handed somebody else's.
    check("the state comes back unchanged", [saved["state"]], query.get("state"))
    code = (query.get("code") or [""])[0]
    check("an authorization code came back", True, bool(code))
    if not code:
        return

    print("\nThe token, and what it opens")
    status, _, text = fetch(
        saved["token_endpoint"],
        method="POST",
        body={
            "grant_type": "authorization_code",
            "code": code,
            "redirect_uri": saved["redirect_uri"],
            "client_id": saved["client_id"],
            "code_verifier": saved["verifier"],
        },
    )
    check("the code exchanges for a token", 200, status)
    tokens = as_json(text, {})
    access, refresh = tokens.get("access_token"), tokens.get("refresh_token")
    if not access:
        print(f"          {text[:200]}")
        return

    claims = json.loads(base64.urlsafe_b64decode(access.split(".")[1] + "==").decode())
    print(f"          aud:       {claims.get('aud')}")
    print(f"          client_id: {claims.get('client_id')}")

    # The audience is what stops this being spendable as a browser session, and
    # naming the client is what stops one grant's tokens being spent as another's.
    check(
        "the audience names this deployment's MCP resource",
        True,
        str(claims.get("aud", "")).startswith(f"{BASE}/mcp"),
    )
    check("and names the client holding it", saved["client_id"], claims.get("client_id"))

    status, _, _ = initialize(BASE, access)
    if status == 403:
        # The credential is good and belongs to nobody here yet. Distinguished
        # because it is the one refusal a person can act on, and because
        # authenticating again cannot fix it.
        note(
            "no Forma account for this identity",
            "sign in and claim the workspace, then re-run",
        )
    else:
        check("the token opens /mcp", 200, status)

    if refresh:
        status, _, text = fetch(
            saved["token_endpoint"],
            method="POST",
            body={
                "grant_type": "refresh_token",
                "refresh_token": refresh,
                "client_id": saved["client_id"],
            },
        )
        check("refreshing returns a new token", 200, status)
        renewed = as_json(text, {}).get("access_token")
        if renewed:
            status, _, _ = initialize(BASE, renewed)
            if status != 403:
                check("and the refreshed one still opens /mcp", 200, status)

    note("revoke this connection from Forma's Agents page")
    note("then re-run the first half: it must not reconnect without a new approval")


if RESUME:
    exchange(RESUME)
    print(f"\n{passed} passed, {failed} failed, {manual} for a person")
    sys.exit(1 if failed else 0)


# --- 1 and 2: the challenge -------------------------------------------------

print("The 401 that starts a discovery")

# Deliberately not the shared `initialize` helper: this one carries no
# credential, and the refusal happens before the body is looked at. What is being
# tested is the challenge, not the handshake.
status, headers, _ = fetch(
    f"{BASE}/mcp",
    method="POST",
    body={"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
    headers={"content-type": "application/json", "accept": "application/json, text/event-stream"},
)
check("an unauthenticated /mcp is refused", 401, status)

challenge = header(headers, "www-authenticate")
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

if not client_id:
    note("everything past registration")
else:
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

    save(
        {
            "client_id": client_id,
            "redirect_uri": redirect_uri,
            "verifier": verifier,
            "state": state,
            "token_endpoint": server.get("token_endpoint", ""),
        }
    )

    note("open this and approve it")
    print(f"\n{authorize}\n")
    print("  It must land on Forma's own consent page, name the client, and return")
    print(f"  to {redirect_uri} carrying ?code= and ?state=. Nothing listens there,")
    print("  so the browser will report a refused connection — the address bar is")
    print("  what matters. Copy the whole URL and run:\n")
    print(f"    ./scripts/oauth-flow.py {BASE} '<the redirected URL>'\n")
    note("the consent page renders and names the client")
    note("approving returns the code and the matching state")

print(f"\n{passed} passed, {failed} failed, {manual} for a person")
sys.exit(1 if failed else 0)
