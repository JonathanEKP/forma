# Sanitized platform outcomes

**Step 12, 2026-10-04.** This note is suitable for the platform team. It contains no service address,
credential, real user data, or application-specific design content.

## Scope

The Step 11 and Step 12 local integration runs exercised a TypeScript application sidecar composed from the
public Gateway, Control Plane and device/runtime packages, with an authenticated host boundary and a real
Codex provider in `capability_only` mode. The provider was Codex 0.158.0 on Linux x86_64, using `gpt-6-luna`
at low reasoning effort. This is evidence for that exact accepted provider row and configuration, not a claim
about other versions, operating systems, models or effort levels.

## Observed outcomes

- The Runtime accepted all seven posture checks and started an application session without relaxing the
  verifier or changing the provider's native-tool policy.
- The provider completed a bounded read and a low-risk write through the registered application
  capabilities. The write was read back from the host domain service, produced one version increment, and
  was attributed to the agent.
- A destructive operation paused for person approval; the host state was unchanged while approval was
  pending, and one approval produced one mutation. Real-Codex denial left state unchanged, and replaying an
  approval resolution did not produce a second mutation. The single expiry attempt observed the approval
  expire and the turn end, but did not observe the expected Runtime refusal diagnostic for a later resolution;
  its post-resolution no-write assertions were not reached, so expiry remains unconfirmed.
- A typed UI action reached the connected application window and completed only after its acknowledgement.
  After an account switch, a fresh session in the new account also completed `ui.openDesign` through the new
  window registration.
- Cancelling while a real Codex `design.validate` MCP request was in flight cancelled the same Runtime
  invocation. Boundary F abandoned the request before the host service answered, the turn ended as
  `turn.cancelled`, and no domain write occurred. A follow-up turn succeeded in the same session. The
  Runtime reported the matching invocation as cancelled, and its workspace was removed when the Device shut
  down. The same cancellation path behaves differently with the deterministic fake provider: it may close
  the provider result channel before the Runtime writes a terminal event. The real-provider observation
  does not establish that the Runtime cancellation defect is fixed for all providers.
- Switching accounts in the same browser tab prevented the new account from restoring or driving the old
  session. When the original account drove its old session again, it returned only its own account data; the
  new account received a distinct session and its UI action was handled. The old Runtime session itself
  remained until its Device shut down because Pontis exposes no per-session stop intent.
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
- Real-Codex approval expiry remains unconfirmed. The one stale-resolution attempt did not produce the
  expected Runtime refusal diagnostic, and the test did not reach the post-resolution mutation assertions;
  the attempt was recorded without retry.
- Pontis has no per-session stop intent. Switching accounts ends the old session for that browser
  window and prevents the new account from reusing it, but the old Runtime session remains until its
  Device shuts down.
