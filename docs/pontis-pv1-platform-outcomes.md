# Sanitized platform outcomes

**Draft pending Step 12 gap runs.** This note is suitable for the platform team after the S6, S8 and S12
records are added. It contains no service address, credential, real user data, or application-specific design
content.

## Scope

One local integration run exercised a TypeScript application sidecar composed from the public Gateway,
Control Plane and device/runtime packages, with an authenticated host boundary and a real Codex provider in
`capability_only` mode. The provider was Codex 0.158.0 on Linux x86_64, using `gpt-6-luna` at low reasoning
effort. This is evidence for that exact accepted provider row and configuration, not a claim about other
versions, operating systems, models or effort levels.

## Observed outcomes

- The Runtime accepted all seven posture checks and started an application session without relaxing the
  verifier or changing the provider's native-tool policy.
- The provider completed a bounded read and a low-risk write through the registered application
  capabilities. The write was read back from the host domain service, produced one version increment, and
  was attributed to the agent.
- A destructive operation paused for person approval; the host state was unchanged while approval was
  pending, and one approval produced one mutation.
- A typed UI action reached the connected application window and completed only after its acknowledgement.
- In one real-provider run, cancelling while a capability was in flight ended the turn without a domain
  write. The same cancellation path behaves differently with the deterministic fake provider: it may close
  the provider result channel before the Runtime writes a terminal event. The real-provider observation
  does not establish that the Runtime cancellation defect is fixed for all providers.
- An out-of-scope request to read a local file, run a shell command, or generate an image resulted in no
  file read, shell action, or domain write, and the provider declined the request. The posture checks and
  provider's native boundary establish the enforcement claim; the reply alone does not.

## Remaining platform limits

- The visual feedback loop is not validated: image-result delivery to the model is unavailable pending
  Pontis CR-2.3 (`pv1_visual_ready`). Existing render evidence shows that the host can render for a person,
  not that the model can inspect the result.
- Long sessions are not validated. A session delegation expires after about five minutes and the Runtime
  refuses it before its refresh path can run; this blocks the long-session acceptance until the Runtime is
  fixed.
- Pontis has no per-session stop intent. Switching accounts ends the old session for that browser
  window and prevents the new account from reusing it, but the old Runtime session remains until its
  Device shuts down.
- Step 12 has not yet recorded real-Codex denial, approval expiry and replay, in-flight MCP-request
  cancellation, or account-switch results. This draft must be updated from those records before it is carried
  to the platform team.
