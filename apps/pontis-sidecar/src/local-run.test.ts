/**
 * The sidecar's local run against FakeProvider (plan §11 step 7).
 *
 * Nothing in the chain is a stand-in except the model:
 *
 * ```text
 * this test (the person) ── Forma session ──▶ sidecar ── /service/resolve ──▶ forma-server
 *                         ── intents ───────▶ sidecar ── device socket ──▶ test-driver
 *                                                                          (Pontis Runtime
 *                                                                           + FakeProvider)
 *                         test-driver ── delegation ──▶ sidecar gateway ── Boundary F ──▶ forma-server
 * ```
 *
 * forma-server is the real binary in hosted mode, verifying session tokens
 * against a JWKS this test serves from `fixtures/identity`. The sidecar is the
 * real `src/main.ts` process. The driver is the Pontis Runtime linked from its
 * public crates (`test-driver/`). The test plays the person: it starts the
 * session, sends input, cancels and answers approvals through the sidecar, and
 * reads what happened from the durable event stream and from Forma.
 *
 * The processes and the person are `stack.ts`, shared with the real-Codex run
 * (`real-codex.test.ts`). It waits on conditions, never on a clock: every wait
 * polls for an event or a state, under a deadline that only turns a hang into a
 * failure.
 */
import { strict as assert } from 'node:assert';
import { randomUUID } from 'node:crypto';
import { after, before, describe, it } from 'node:test';

import { approvalId, documentOf, json, Stack, type Design } from './stack.ts';

/** The inputs test-driver/src/main.rs scripts, byte for byte. */
const READ = 'Which projects do I have, and what is in my design?';
const WRITE = 'Raise the headline to 96pt.';
const CANCEL = 'Validate the long design.';
const DENY = 'Remove the tagline. I will say no.';
const APPROVE = 'Remove the headline.';
const GUIDE = 'Read the manual first.';
const BRAND = 'Read my brand, and ask me for a hero photo.';
const LIST = 'List the designs of my project.';

/** Pages in the design whose validation is cancelled: enough that it is still running when the cancel lands. */
const LONG_PAGES = 120;

const stack = new Stack();
let design: Design | undefined;
let layer = '';

describe('the sidecar, run locally against FakeProvider', () => {
  before(async () => {
    await stack.boot('Local run');
    layer = randomUUID();
    const tagline = randomUUID();
    design = await stack.createDesign('Cover', documentOf(1, [layer, tagline]));
    const longDesign = (await stack.createDesign('Long', documentOf(LONG_PAGES, [randomUUID()]))).id;
    await stack.attachDriver(
      {
        FORMA_DRIVER_PROJECT: stack.projectId,
        FORMA_DRIVER_DESIGN: design.id,
        FORMA_DRIVER_LONG_DESIGN: longDesign,
        FORMA_DRIVER_LAYER: layer,
        FORMA_DRIVER_TAGLINE: tagline,
        FORMA_DRIVER_BASE_VERSION: String(design.version),
      },
      'fake',
    );
  });

  after(async () => {
    await stack.close();
  });

  it('reads: the project list and a design, through Boundary F', async () => {
    const events = await stack.turn(READ, 'turn.completed');
    const completed = events.filter((event) => event.type === 'capability.completed');
    assert.deepEqual(completed.map((event) => event.payload.capabilityId), ['project.list', 'design.get']);
    const [projects, read] = completed.map((event) => JSON.stringify(event.payload.result));
    assert.ok(projects?.includes('Local run'), projects);
    assert.ok(read?.includes(layer), 'the design comes back with its headline');
  });

  it('lists the designs of a project, so the agent can learn an id it was not given (design.list)', async () => {
    const events = await stack.turn(LIST, 'turn.completed');
    const completed = events.filter((event) => event.type === 'capability.completed');
    assert.deepEqual(completed.map((event) => event.payload.capabilityId), ['design.list']);
    const { designs } = completed[0]?.payload.result as { designs: { id: string; name: string; page_count: number }[] };
    assert.deepEqual(designs.map(({ name }) => name).sort(), ['Cover', 'Long']);
    assert.equal(designs.find(({ id }) => id === design?.id)?.page_count, 1);
    assert.ok(!JSON.stringify(designs).includes('"document"'), 'a listing carries no document');
    assert.equal(stack.boundaryCalls('design.list').filter((line) => line.status === 200).length, 1);
  });

  it('reads the manual: forma.guide answers the Pontis rendering and its revision, through Boundary F', async () => {
    const events = await stack.turn(GUIDE, 'turn.completed');
    const completed = events.filter((event) => event.type === 'capability.completed');
    assert.deepEqual(completed.map((event) => event.payload.capabilityId), ['forma.guide']);
    const result = completed[0]?.payload.result as { revision?: unknown; text?: unknown } | undefined;
    assert.deepEqual(Object.keys(result ?? {}).sort(), ['revision', 'text'], 'no timestamp, no capability list');
    assert.match(String(result?.revision), /^blake3:[0-9a-f]{64}$/u);
    const text = String(result?.text);
    // Rendered for this door: capability ids, never the MCP door's tool names.
    assert.ok(text.includes('`design.applyOperations`'));
    assert.ok(!text.includes('apply_operations') && !text.includes('render_preview'));
    assert.equal(stack.boundaryCalls('forma.guide').filter((line) => line.status === 200).length, 1);
  });

  it('reads the brand with its font catalog, and asks for an image, through Boundary F', async () => {
    const events = await stack.turn(BRAND, 'turn.completed');
    const completed = events.filter((event) => event.type === 'capability.completed');
    assert.deepEqual(completed.map((event) => event.payload.capabilityId), ['brand.get', 'assetRequest.create']);
    const brand = completed[0]?.payload.result as { brand?: unknown; fonts?: unknown } | undefined;
    // No kit on this project, and the catalog anyway: it is what the agent
    // uses in place of $brand.* tokens.
    assert.equal(brand?.brand, null);
    assert.ok(Array.isArray(brand?.fonts) && brand.fonts.includes('Geist'), JSON.stringify(brand?.fonts));
    // One request, pending, in Forma itself.
    const requests = (await json(await stack.asPerson('GET', `/api/v1/projects/${stack.projectId}/asset-requests`))) as { status: string; purpose: string }[];
    assert.deepEqual(requests.map(({ status, purpose }) => [status, purpose]), [['pending', 'hero photo']]);
  });

  it('writes: one batch, one new version, applied by Forma', async () => {
    const events = await stack.turn(WRITE, 'turn.completed');
    assert.ok(events.some((event) => event.type === 'capability.completed' && event.payload.capabilityId === 'design.applyOperations'));
    const now = await stack.read(design!.id);
    assert.equal(now.version, design!.version + 1);
    assert.equal(headline(now)?.font_size, 96);
  });

  it('cancels: the Runtime abandons validate mid-flight, and the sidecar drops the Boundary F request', async (t) => {
    const sent = stack.cursor;
    await stack.say(CANCEL);
    await stack.nextEvent('capability.requested', (event) => event.payload.capabilityId === 'design.validate');
    await stack.until('the sidecar to be validating', () => Promise.resolve(stack.boundaryCalls('design.validate', 'boundary_f_request').length > 0));
    await stack.intent('session.cancel', { sessionId: stack.sessionId });

    // The Runtime gives up on the gateway call…
    const failed = await stack.nextEvent('capability.failed', (event) => event.payload.capabilityId === 'design.validate');
    assert.match(JSON.stringify(failed.at(-1)?.payload), /approval_cancelled/u);
    assert.ok(!failed.some((event) => event.sequence > sent && event.type === 'capability.completed'), 'validation did not complete');
    // …the sidecar abandons Boundary F before Forma answers, which is the
    // signal forma-server's validate stops on (`cancel_on_drop`, tested there).
    await stack.until('the sidecar to abandon validate', () =>
      Promise.resolve(stack.boundaryCalls('design.validate').some((line) => line.outcome === 'aborted')),
    );
    assert.ok(!stack.boundaryCalls('design.validate').some((line) => line.status === 200), 'Forma never answered the validation');

    // …and the turn ends. **Pontis finding:** with FakeProvider, cancelling
    // while a capability is in flight ends the provider's turn and closes its
    // result channel before the Runtime submits the cancelled outcome; that
    // submit fails, `send_input` returns early, and the provider's
    // `turn.cancelled` is never read, so no terminal durable event is written
    // and the error reaches only the RemoteAgent's observer. Either ending is
    // accepted here, so this test keeps passing when Pontis fixes it.
    await stack.until('the turn to end', () =>
      Promise.resolve(
        stack.seen.some((event) => event.sequence > sent && event.type === 'turn.cancelled') ||
          (stack.children.find(({ name }) => name === 'test-driver')?.stderr.includes('REFUSED provider_exited') ?? false),
      ),
    );
    t.diagnostic(
      stack.seen.some((event) => event.sequence > sent && event.type === 'turn.cancelled')
        ? 'the turn ended with turn.cancelled'
        : 'the turn ended with no terminal event; the Runtime refused: provider_exited (the Pontis finding above)',
    );
  });

  it('asks before removing anything, and a refusal writes nothing', async () => {
    const before = await stack.read(design!.id);
    const applies = stack.boundaryCalls('design.applyOperations').length;
    await stack.say(DENY);
    const pending = await stack.nextEvent('approval.requested');
    // Consent is outstanding: nothing has reached Forma, and nothing changed.
    assert.equal(stack.boundaryCalls('design.applyOperations').length, applies, 'no Boundary F write before consent');
    assert.deepEqual(await stack.read(design!.id), before);
    await stack.intent('approval.resolve', { approvalId: approvalId(pending), decision: 'deny' });
    const settled = await stack.nextEvent('turn.completed');
    assert.ok(settled.some((event) => event.type === 'capability.failed' && JSON.stringify(event.payload).includes('approval_denied')));
    assert.equal(stack.boundaryCalls('design.applyOperations').length, applies, 'no Boundary F write after a refusal');
    assert.deepEqual(await stack.read(design!.id), before);
  });

  it('removes only once the person approves, as exactly one new version', async () => {
    const before = await stack.read(design!.id);
    const applies = stack.boundaryCalls('design.applyOperations').length;
    await stack.say(APPROVE);
    const pending = await stack.nextEvent('approval.requested');
    assert.equal(stack.boundaryCalls('design.applyOperations').length, applies, 'no Boundary F write before consent');
    assert.deepEqual(await stack.read(design!.id), before, 'the design is untouched while consent is pending');
    await stack.intent('approval.resolve', { approvalId: approvalId(pending), decision: 'approveOnce' });
    await stack.nextEvent('turn.completed');
    const now = await stack.read(design!.id);
    assert.equal(now.version, before.version + 1);
    assert.equal(headline(now), undefined, 'the headline is gone');
    assert.equal(stack.boundaryCalls('design.applyOperations').length, applies + 1, 'one write, carrying the approval');
  });
});


function headline(record: Design): { id: string; font_size?: number } | undefined {
  return record.document.pages.flatMap((page) => page.layers).find((candidate) => candidate.id === layer);
}
