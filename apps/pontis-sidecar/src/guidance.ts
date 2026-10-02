/**
 * A stand-in for `ApplicationAgentGuidance`, which Pontis requires before it
 * will register an application at all (ADR-027 Decision 2).
 *
 * **Not the guidance.** Writing that is plan §11 step 8: a distillation of the
 * skill for owner review, with its own drift guard, and under `capability_only`
 * it is the only documentation the agent receives (§6.1). This is the smallest
 * honest value that lets the composition run against FakeProvider, which reads
 * none of it. Nothing ships until step 8 replaces it.
 */
import type { ApplicationAgentGuidance } from '@pontis/core';

export const PLACEHOLDER_GUIDANCE: ApplicationAgentGuidance = {
  version: 1,
  role: 'Forma design assistant',
  purpose: 'Help the person inspect and edit their Forma designs through the registered capabilities.',
  instructions: [
    'This is placeholder guidance for local integration runs; the reviewed guidance replaces it before any real session.',
    'Read a design before editing it, and pass the version you read as base_version.',
  ],
  outOfScope: ['Decline tasks unrelated to the person’s Forma designs.'],
  terminology: [],
};
