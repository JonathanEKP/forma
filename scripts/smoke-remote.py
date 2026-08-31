#!/usr/bin/env python3
"""External acceptance for the remote deploy, run from off the machine.

    FORMA_REMOTE_TOKEN=... ./scripts/smoke-remote.py https://forma.hintly.online

Fetch the token without printing it:

    export FORMA_REMOTE_TOKEN=$(ssh -i ~/.ssh/id_rsa2 ubuntu@HOST \\
        "sudo sed -n 's/^FORMA_REMOTE_TOKEN=//p' /etc/forma/forma.env")

Plan section 77's acceptance for phase 11 is "an authorized remote MCP host can
create/render a piece", so that is what this does -- over the real domain, over
TLS, through Caddy -- and it deletes what it made.

Python rather than shell because the MCP replies are JSON inside server-sent
events. The first version of this parsed them with sed and returned the wrong id
on its first real run, which is the argument.
"""

import json
import os
import sys
import urllib.error
import urllib.request

BASE = (sys.argv[1] if len(sys.argv) > 1 else "").rstrip("/")
TOKEN = os.environ.get("FORMA_REMOTE_TOKEN", "")

if not BASE or not TOKEN:
    print(__doc__.strip().splitlines()[2].strip(), file=sys.stderr)
    sys.exit(2)

passed, failed = 0, 0


def check(name, expected, actual):
    global passed, failed
    if expected == actual:
        print(f"  ok    {name:<46} {actual}")
        passed += 1
    else:
        print(f"  FAIL  {name:<46} got {actual!r}, wanted {expected!r}")
        failed += 1


def request(url, *, method="GET", body=None, headers=None, token=True, timeout=120):
    """Returns (status, body-text). A refusal is an answer, not an exception."""
    head = {"Content-Type": "application/json"}
    if token:
        head["Authorization"] = f"Bearer {TOKEN}"
    head.update(headers or {})
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, headers=head, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            return response.status, response.read().decode("utf-8", "replace"), response.headers
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode("utf-8", "replace"), error.headers
    except Exception as error:  # noqa: BLE001 - a connection refusal is a result here
        return 0, str(error), {}


def status(url, **kwargs):
    return request(url, **kwargs)[0]


session = {"id": None}


def mcp(method, params=None, request_id=None):
    """One MCP call. Replies arrive as JSON inside server-sent events."""
    body = {"jsonrpc": "2.0", "method": method}
    if request_id is not None:
        body["id"] = request_id
    if params is not None:
        body["params"] = params

    headers = {"Accept": "application/json, text/event-stream"}
    if session["id"]:
        headers["mcp-session-id"] = session["id"]

    code, text, head = request(f"{BASE}/mcp", method="POST", body=body, headers=headers)
    if head and head.get("mcp-session-id"):
        session["id"] = head["mcp-session-id"]

    for line in text.splitlines():
        if line.startswith("data: {"):
            return code, json.loads(line[len("data: "):])
    try:
        return code, json.loads(text)
    except json.JSONDecodeError:
        return code, {"raw": text[:200]}


def tool(name, arguments, request_id):
    code, reply = mcp("tools/call", {"name": name, "arguments": arguments}, request_id)
    result = reply.get("result", {})
    if result.get("isError"):
        return code, result, None
    return code, result, result.get("structuredContent")


print("TLS and routing")
# The bundle is public and the data is not (plan section 110.1). This replaces
# the Phase 11 assertion that the whole origin refused: `/login` exists to be
# reached before you have a credential, and a provider sends a person to
# `/oauth/consent` holding none at all. It is a weaker claim, stated honestly,
# rather than the same claim quietly kept.
check("the app shell is public", 200, status(f"{BASE}/", token=False))
check("and so are its client routes", 200, status(f"{BASE}/login", token=False))
check("the skill is public", 200, status(f"{BASE}/skill", token=False))
check("health answers without a token", 200, status(f"{BASE}/api/v1/health", token=False))

print("\nThe credential")
check("api refuses without a token", 401, status(f"{BASE}/api/v1/projects", token=False))
check(
    "api refuses a wrong token",
    401,
    status(f"{BASE}/api/v1/projects", token=False, headers={"Authorization": "Bearer wrong"}),
)
check("api accepts the token", 200, status(f"{BASE}/api/v1/projects"))
check("mcp refuses without a token", 401, status(f"{BASE}/mcp", method="POST", token=False))

print("\nPorts that must not be reachable")
host = BASE.split("://", 1)[1].split("/", 1)[0]
for port in (4765, 4766):
    code, _, _ = request(f"http://{host}:{port}/", token=False, timeout=5)
    check(f"{port} is not exposed", 0, code)

print("\nMCP over HTTP")
_, reply = mcp("initialize", {
    "protocolVersion": "2025-06-18",
    "capabilities": {},
    "clientInfo": {"name": "smoke", "version": "0"},
}, 1)
check("initialize", "forma-mcp", reply.get("result", {}).get("serverInfo", {}).get("name"))
mcp("notifications/initialized")

_, _, info = tool("forma_status", {}, 2)
check("the token reaches the REST API", "forma-server", (info or {}).get("name"))

print("\nPhase 11 acceptance: an authorized host creates and renders a piece")
code, text, _ = request(
    f"{BASE}/api/v1/projects", method="POST", body={"name": "Deploy smoke test"}
)
project = json.loads(text)["id"] if code in (200, 201) else None
check("a project to work in", True, project is not None)

if project:
    try:
        _, _, record = tool("create_design", {"project_id": project, "name": "Smoke"}, 3)
        check("an agent creates a design", 1, (record or {}).get("version"))

        design_id = (record or {}).get("id")
        _, result, _ = tool("render_preview", {"design_id": design_id}, 4)
        kinds = [block.get("type") for block in result.get("content", [])]
        check("and renders it, as an image block", True, "image" in kinds)
    finally:
        # A smoke test that leaves rubbish in someone's workspace is a smoke
        # test people stop running.
        gone = status(f"{BASE}/api/v1/projects/{project}", method="DELETE")
        check("cleaned up after itself", 204, gone)

print(f"\n{passed} passed, {failed} failed")
sys.exit(1 if failed else 0)
