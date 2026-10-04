/**
 * The guidance, held to ADR-027 Decision 2 by Pontis's own validator rather
 * than by inspection, and to the capabilities it names.
 *
 * Those are held to the **registered** set — what `formaCapabilities` builds
 * and the sidecar publishes — not to the generated catalog. The catalog lists
 * every operation Forma defines on this door; the agent can call only what is
 * registered, and guidance naming the rest sends it to `capability_not_found`
 * with no way to recover. A check against the catalog passed exactly that.
 */
import { strict as assert } from 'node:assert';
import { describe, it } from 'node:test';

import { GUIDANCE_BOUNDS, canonicalGuidanceJson, validateApplicationAgentGuidance } from '@pontis/core';

import { FormaBoundary } from './boundary.ts';
import { formaCapabilities } from './capabilities.ts';
import { GUIDANCE } from './guidance.ts';
import { silentLog } from './log.ts';
import { UI_ACTIONS } from './ui.ts';

/** What a session can actually call: Boundary F's operations and the editor's four actions. */
const REGISTERED = new Set<string>([
  ...formaCapabilities(new FormaBoundary('http://127.0.0.1:1', 'unused', silentLog)).map(({ id }) => id),
  ...UI_ACTIONS,
]);

const bytes = (text: string): number => Buffer.byteLength(text, 'utf8');

/** Every string an agent reads, with where it sits. */
function prose(): [string, string][] {
  return [
    ['role', GUIDANCE.role],
    ['purpose', GUIDANCE.purpose],
    ...GUIDANCE.instructions.map((text, i): [string, string] => [`instructions[${i}]`, text]),
    ...GUIDANCE.outOfScope.map((text, i): [string, string] => [`outOfScope[${i}]`, text]),
    ...GUIDANCE.terminology.flatMap(({ term, meaning }, i): [string, string][] => [
      [`terminology[${i}].term`, term],
      [`terminology[${i}].meaning`, meaning],
    ]),
  ];
}

describe('the guidance', () => {
  it('is valid ADR-027 guidance, by the validator the Gateway runs', () => {
    // Throws GuidanceValidationError naming the field on any breach: a missing
    // or unknown field, a bound, a control or format character, an edge space.
    validateApplicationAgentGuidance(GUIDANCE);

    const canonical = bytes(canonicalGuidanceJson(GUIDANCE));
    assert.ok(canonical <= GUIDANCE_BOUNDS.canonicalBytes, `${canonical} canonical bytes`);
    // Reported, so a reviewer sees how much of the budget the opening context
    // spends without having to measure it.
    console.log(
      `guidance: ${canonical} of ${GUIDANCE_BOUNDS.canonicalBytes} canonical bytes; ` +
        `${GUIDANCE.instructions.length} instructions, ${GUIDANCE.outOfScope.length} out of scope, ` +
        `${GUIDANCE.terminology.length} terms; longest instruction ` +
        `${Math.max(...GUIDANCE.instructions.map(bytes))} of ${GUIDANCE_BOUNDS.instructionBytes} bytes`,
    );
  });

  it('is refused by that validator one byte past a bound, so the check above is not vacuous', () => {
    assert.throws(() => validateApplicationAgentGuidance({ ...GUIDANCE, role: 'x'.repeat(GUIDANCE_BOUNDS.roleBytes + 1) }));
    assert.throws(() => validateApplicationAgentGuidance({ ...GUIDANCE, instructions: [...GUIDANCE.instructions, ' padded'] }));
  });

  it('points at the manual exactly once, and names only capabilities the session can call', () => {
    const pointers = GUIDANCE.instructions.filter((text) => text.includes('forma.guide'));
    assert.equal(pointers.length, 1, 'one instruction says where the manual is');

    // A capability id is dotted camelCase; a sentence's full stop is followed
    // by a space, so it never matches.
    // Every miss at once, so a failure lists the whole of what is missing.
    const unreachable = prose().flatMap(([where, text]) =>
      [...text.matchAll(/\b[a-z][a-zA-Z0-9]*(?:\.[a-z][a-zA-Z0-9]*)+\b/gu)]
        .map(([named]) => named)
        .filter((named) => !REGISTERED.has(named))
        .map((named) => `${where} names ${named}`),
    );
    assert.deepEqual(unreachable, [], 'the guidance names capabilities the sidecar does not register');
  });

  it('explains that a timed-out UI action may already have happened', () => {
    const timeout = GUIDANCE.instructions.filter((text) => text.includes('ui_action_timeout'));
    assert.equal(timeout.length, 1);
    assert.match(timeout[0] ?? '', /outcome is unknown; check the editor before repeating it/u);
  });

  it('carries no manual revision and no MCP tool name', () => {
    // A revision here would change the guidance digest whenever the manual
    // changed; an MCP name would send a Pontis agent to a tool it does not have.
    for (const [where, text] of prose()) {
      assert.ok(!/blake3:|sha256:/u.test(text), `${where} carries a revision`);
      assert.ok(!/\b(?:get|list|create|apply|request|validate|render|export)_[a-z_]+\b|\bforma_status\b/u.test(text), `${where}: ${text}`);
    }
  });
});
