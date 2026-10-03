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
 * It waits on conditions, never on a clock: every wait polls for an event or a
 * state, under a deadline that only turns a hang into a failure.
 */
import { strict as assert } from 'node:assert';
import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { createPrivateKey, randomUUID, sign } from 'node:crypto';
import { once } from 'node:events';
import { readFileSync } from 'node:fs';
import { mkdtemp, rm } from 'node:fs/promises';
import { createServer, type Server } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { after, before, describe, it } from 'node:test';

const PACKAGE = resolve(import.meta.dirname, '..');
const REPO = resolve(PACKAGE, '../..');
const FORMA_SERVER = process.env.FORMA_SERVER_BIN ?? join(REPO, 'target/debug/forma-server');
const DRIVER = process.env.FORMA_PONTIS_DRIVER_BIN ?? join(PACKAGE, 'test-driver/target/debug/forma-pontis-test-driver');

const ISSUER = 'https://project.supabase.co/auth/v1';
const SIGNING_KEY = readFileSync(join(REPO, 'fixtures/identity/test-signing-key.pkcs8.der'));
const JWKS = readFileSync(join(REPO, 'fixtures/identity/test-jwks.json'));

/** The inputs test-driver/src/main.rs scripts, byte for byte. */
const READ = 'Which projects do I have, and what is in my design?';
const WRITE = 'Raise the headline to 96pt.';
const CANCEL = 'Validate the long design.';
const DENY = 'Remove the tagline. I will say no.';
const APPROVE = 'Remove the headline.';
const GUIDE = 'Read the manual first.';
const BRAND = 'Read my brand, and ask me for a hero photo.';

/** Pages in the design whose validation is cancelled: enough that it is still running when the cancel lands. */
const LONG_PAGES = 120;

const DEADLINE_MS = 60_000;

interface Child {
  readonly process: ChildProcess;
  readonly name: string;
  stdout: string;
  stderr: string;
}

interface DurableEvent {
  readonly sequence: number;
  readonly type: string;
  readonly sessionId?: string;
  readonly payload: Record<string, unknown>;
}

interface Design {
  readonly id: string;
  readonly version: number;
  readonly document: { readonly pages: readonly { readonly layers: readonly { readonly id: string; readonly font_size?: number }[] }[] };
}

const children: Child[] = [];
const servers: Server[] = [];
let directory = '';

const SUBJECT = randomUUID();
const SESSION_TOKEN = mintSession(SUBJECT);
let credential = '';
let account = '';
let forma = '';
let sidecar = '';
let projectId = '';
let design: Design | undefined;
let longDesign = '';
let layer = '';
let tagline = '';
let sessionId = '';
let cursor = 0;

describe('the sidecar, run locally against FakeProvider', () => {
  before(async () => {
    for (const [binary, how] of [
      [FORMA_SERVER, 'cargo build -p forma-server'],
      [DRIVER, 'cargo build --manifest-path apps/pontis-sidecar/test-driver/Cargo.toml'],
    ] as const) {
      assert.ok(spawnSync(binary, ['--help']).error === undefined, `${binary} is missing: ${how} (scripts/check.sh does this)`);
    }
    directory = await mkdtemp(join(tmpdir(), 'forma-pontis-'));
    const environment = formaEnvironment(await serveJwks(), await freePort());

    // An account for the person, and the sidecar's credential — both minted by
    // the operator commands a deployment uses.
    const granted = run(FORMA_SERVER, ['grant-owner', 'person@example.com', SUBJECT], environment);
    account = /^account (\S+)$/mu.exec(granted)?.[1] ?? assert.fail(`no account in ${granted}`);
    credential = run(FORMA_SERVER, ['service-credential', 'local-run'], environment).trim();
    assert.match(credential, /^forma_svc_/u);

    const server = start('forma-server', FORMA_SERVER, [], environment);
    forma = `http://${environment.FORMA_BIND}`;
    await until('forma-server to answer', async () => (await asPerson('GET', '/api/v1/status').catch(() => undefined))?.ok === true, server);

    const project = (await json(await asPerson('POST', '/api/v1/projects', { name: 'Local run' }))) as { id: string };
    projectId = project.id;
    layer = randomUUID();
    tagline = randomUUID();
    design = (await json(
      await asPerson('POST', `/api/v1/projects/${project.id}/designs`, {
        name: 'Cover',
        format: null,
        document: documentOf(1, [layer, tagline]),
      }),
    )) as Design;
    longDesign = ((await json(
      await asPerson('POST', `/api/v1/projects/${project.id}/designs`, {
        name: 'Long',
        format: null,
        document: documentOf(LONG_PAGES, [randomUUID()]),
      }),
    )) as Design).id;

    const sidecarProcess = start('sidecar', process.execPath, [join(PACKAGE, 'src/main.ts')], {
      PATH: process.env.PATH ?? '',
      FORMA_SERVER_URL: forma,
      FORMA_SERVICE_CREDENTIAL: credential,
      FORMA_PONTIS_BIND: '127.0.0.1:0',
    });
    sidecar = await ready(sidecarProcess);

    const pairing = (await json(await toSidecar('POST', '/pons/v1/pairing/codes', {}))) as { code: string; applicationId: string };
    const driver = start('test-driver', DRIVER, [], {
      PATH: process.env.PATH ?? '',
      FORMA_PONTIS_ORIGIN: sidecar,
      FORMA_PONTIS_APPLICATION_ID: pairing.applicationId,
      FORMA_DRIVER_PAIRING_CODE: pairing.code,
      FORMA_DRIVER_PROJECT: project.id,
      FORMA_DRIVER_DESIGN: design.id,
      FORMA_DRIVER_LONG_DESIGN: longDesign,
      FORMA_DRIVER_LAYER: layer,
      FORMA_DRIVER_TAGLINE: tagline,
      FORMA_DRIVER_BASE_VERSION: String(design.version),
    });
    await ready(driver);

    const started = (await json(await toSidecar('POST', '/agent/intent', { type: 'session.start', payload: { providerId: 'fake' } }))) as {
      sessionId: string;
      delivered: boolean;
    };
    assert.equal(started.delivered, true);
    sessionId = started.sessionId;
    await nextEvent('session.started');
  });

  after(async () => {
    await Promise.allSettled(children.map(stop));
    await Promise.allSettled(servers.map((server) => new Promise((done) => server.close(done))));
    if (directory) await rm(directory, { recursive: true, force: true });
    const output = JSON.stringify(children.map(({ stdout, stderr }) => [stdout, stderr]));
    // Checked last, over everything every process printed.
    assert.ok(!output.includes(credential), 'the service credential never reaches any output');
    assert.ok(!output.includes(SESSION_TOKEN), 'the session token never reaches any output');
  });

  it('reads: the project list and a design, through Boundary F', async () => {
    const events = await turn(READ, 'turn.completed');
    const completed = events.filter((event) => event.type === 'capability.completed');
    assert.deepEqual(completed.map((event) => event.payload.capabilityId), ['project.list', 'design.get']);
    const [projects, read] = completed.map((event) => JSON.stringify(event.payload.result));
    assert.ok(projects?.includes('Local run'), projects);
    assert.ok(read?.includes(layer), 'the design comes back with its headline');
  });

  it('reads the manual: forma.guide answers the Pontis rendering and its revision, through Boundary F', async () => {
    const events = await turn(GUIDE, 'turn.completed');
    const completed = events.filter((event) => event.type === 'capability.completed');
    assert.deepEqual(completed.map((event) => event.payload.capabilityId), ['forma.guide']);
    const result = completed[0]?.payload.result as { revision?: unknown; text?: unknown } | undefined;
    assert.deepEqual(Object.keys(result ?? {}).sort(), ['revision', 'text'], 'no timestamp, no capability list');
    assert.match(String(result?.revision), /^blake3:[0-9a-f]{64}$/u);
    const text = String(result?.text);
    // Rendered for this door: capability ids, never the MCP door's tool names.
    assert.ok(text.includes('`design.applyOperations`'));
    assert.ok(!text.includes('apply_operations') && !text.includes('render_preview'));
    assert.equal(boundaryCalls('forma.guide').filter((line) => line.status === 200).length, 1);
  });

  it('reads the brand with its font catalog, and asks for an image, through Boundary F', async () => {
    const events = await turn(BRAND, 'turn.completed');
    const completed = events.filter((event) => event.type === 'capability.completed');
    assert.deepEqual(completed.map((event) => event.payload.capabilityId), ['brand.get', 'assetRequest.create']);
    const brand = completed[0]?.payload.result as { brand?: unknown; fonts?: unknown } | undefined;
    // No kit on this project, and the catalog anyway: it is what the agent
    // uses in place of $brand.* tokens.
    assert.equal(brand?.brand, null);
    assert.ok(Array.isArray(brand?.fonts) && brand.fonts.includes('Geist'), JSON.stringify(brand?.fonts));
    // One request, pending, in Forma itself.
    const requests = (await json(await asPerson('GET', `/api/v1/projects/${projectId}/asset-requests`))) as { status: string; purpose: string }[];
    assert.deepEqual(requests.map(({ status, purpose }) => [status, purpose]), [['pending', 'hero photo']]);
  });

  it('writes: one batch, one new version, applied by Forma', async () => {
    const events = await turn(WRITE, 'turn.completed');
    assert.ok(events.some((event) => event.type === 'capability.completed' && event.payload.capabilityId === 'design.applyOperations'));
    const now = await read(design!.id);
    assert.equal(now.version, design!.version + 1);
    assert.equal(headline(now)?.font_size, 96);
  });

  it('cancels: the Runtime abandons validate mid-flight, and the sidecar drops the Boundary F request', async (t) => {
    const sent = cursor;
    await say(CANCEL);
    await nextEvent('capability.requested', (event) => event.payload.capabilityId === 'design.validate');
    await until('the sidecar to be validating', () => Promise.resolve(boundaryCalls('design.validate', 'boundary_f_request').length > 0));
    await intent('session.cancel', { sessionId });

    // The Runtime gives up on the gateway call…
    const failed = await nextEvent('capability.failed', (event) => event.payload.capabilityId === 'design.validate');
    assert.match(JSON.stringify(failed.at(-1)?.payload), /approval_cancelled/u);
    assert.ok(!failed.some((event) => event.sequence > sent && event.type === 'capability.completed'), 'validation did not complete');
    // …the sidecar abandons Boundary F before Forma answers, which is the
    // signal forma-server's validate stops on (`cancel_on_drop`, tested there).
    await until('the sidecar to abandon validate', () =>
      Promise.resolve(boundaryCalls('design.validate').some((line) => line.outcome === 'aborted')),
    );
    assert.ok(!boundaryCalls('design.validate').some((line) => line.status === 200), 'Forma never answered the validation');

    // …and the turn ends. **Pontis finding:** with FakeProvider, cancelling
    // while a capability is in flight ends the provider's turn and closes its
    // result channel before the Runtime submits the cancelled outcome; that
    // submit fails, `send_input` returns early, and the provider's
    // `turn.cancelled` is never read, so no terminal durable event is written
    // and the error reaches only the RemoteAgent's observer. Either ending is
    // accepted here, so this test keeps passing when Pontis fixes it.
    await until('the turn to end', () =>
      Promise.resolve(
        seen.some((event) => event.sequence > sent && event.type === 'turn.cancelled') ||
          (children.find(({ name }) => name === 'test-driver')?.stderr.includes('REFUSED provider_exited') ?? false),
      ),
    );
    t.diagnostic(
      seen.some((event) => event.sequence > sent && event.type === 'turn.cancelled')
        ? 'the turn ended with turn.cancelled'
        : 'the turn ended with no terminal event; the Runtime refused: provider_exited (the Pontis finding above)',
    );
  });

  it('asks before removing anything, and a refusal writes nothing', async () => {
    const before = await read(design!.id);
    const applies = boundaryCalls('design.applyOperations').length;
    await say(DENY);
    const pending = await nextEvent('approval.requested');
    // Consent is outstanding: nothing has reached Forma, and nothing changed.
    assert.equal(boundaryCalls('design.applyOperations').length, applies, 'no Boundary F write before consent');
    assert.deepEqual(await read(design!.id), before);
    await intent('approval.resolve', { approvalId: approvalId(pending), decision: 'deny' });
    const settled = await nextEvent('turn.completed');
    assert.ok(settled.some((event) => event.type === 'capability.failed' && JSON.stringify(event.payload).includes('approval_denied')));
    assert.equal(boundaryCalls('design.applyOperations').length, applies, 'no Boundary F write after a refusal');
    assert.deepEqual(await read(design!.id), before);
  });

  it('removes only once the person approves, as exactly one new version', async () => {
    const before = await read(design!.id);
    const applies = boundaryCalls('design.applyOperations').length;
    await say(APPROVE);
    const pending = await nextEvent('approval.requested');
    assert.equal(boundaryCalls('design.applyOperations').length, applies, 'no Boundary F write before consent');
    assert.deepEqual(await read(design!.id), before, 'the design is untouched while consent is pending');
    await intent('approval.resolve', { approvalId: approvalId(pending), decision: 'approveOnce' });
    await nextEvent('turn.completed');
    const now = await read(design!.id);
    assert.equal(now.version, before.version + 1);
    assert.equal(headline(now), undefined, 'the headline is gone');
    assert.equal(boundaryCalls('design.applyOperations').length, applies + 1, 'one write, carrying the approval');
  });
});

// --- the person -----------------------------------------------------------------------------

function mintSession(subject: string): string {
  const encode = (value: unknown) => Buffer.from(JSON.stringify(value)).toString('base64url');
  const body = `${encode({ alg: 'ES256', typ: 'JWT', kid: 'test-key-1' })}.${encode({
    sub: subject,
    iss: ISSUER,
    aud: 'authenticated',
    exp: Math.floor(Date.now() / 1000) + 3600,
    email: 'person@example.com',
    email_verified: true,
  })}`;
  const key = createPrivateKey({ key: SIGNING_KEY, format: 'der', type: 'pkcs8' });
  return `${body}.${sign('sha256', Buffer.from(body), { key, dsaEncoding: 'ieee-p1363' }).toString('base64url')}`;
}

async function asPerson(method: string, path: string, body?: unknown): Promise<Response> {
  return await fetch(`${forma}${path}`, request(method, body));
}

async function toSidecar(method: string, path: string, body?: unknown): Promise<Response> {
  return await fetch(`${sidecar}${path}`, request(method, body));
}

function request(method: string, body?: unknown): RequestInit {
  return {
    method,
    headers: { authorization: `Bearer ${SESSION_TOKEN}`, 'content-type': 'application/json' },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  };
}

async function json(response: Response): Promise<unknown> {
  const text = await response.text();
  assert.ok(response.ok, `${response.status} ${response.url}: ${text}`);
  const parsed: unknown = JSON.parse(text);
  return parsed;
}

async function read(id: string): Promise<Design> {
  return (await json(await asPerson('GET', `/api/v1/designs/${id}`))) as Design;
}

function headline(record: Design): { id: string; font_size?: number } | undefined {
  return record.document.pages.flatMap((page) => page.layers).find((candidate) => candidate.id === layer);
}

async function intent(type: string, payload: Record<string, unknown>): Promise<void> {
  const answer = (await json(await toSidecar('POST', '/agent/intent', { type, payload }))) as { delivered: boolean };
  assert.equal(answer.delivered, true, `${type} reached the Runtime`);
}

async function say(text: string): Promise<void> {
  await intent('session.input', { sessionId, text });
}

/** Sends `text` and returns every event of the turn, up to `terminal`. */
async function turn(text: string, terminal: string): Promise<DurableEvent[]> {
  const sent = cursor;
  await say(text);
  const events = await nextEvent(terminal);
  return events.filter((event) => event.sequence > sent);
}

let seen: DurableEvent[] = [];

/** Waits for the next event of `type` past the cursor; returns everything read up to it. */
async function nextEvent(type: string, matches: (event: DurableEvent) => boolean = () => true): Promise<DurableEvent[]> {
  let found: DurableEvent | undefined;
  await until(`${type}`, async () => {
    const page = (await json(
      await toSidecar('GET', `/pons/v1/events?scope=application_context&applicationContextId=${account}&after=0`),
    )) as { events: DurableEvent[] };
    seen = page.events;
    found = seen.find((event) => event.sequence > cursor && event.type === type && (event.sessionId ?? sessionId) === sessionId && matches(event));
    for (const failure of seen.filter((event) => event.sequence > cursor && (event.type === 'turn.failed' || event.type === 'session.failed'))) {
      const causes = seen.filter((event) => event.sequence > cursor && event.type === 'capability.failed');
      assert.fail(`${failure.type}: ${JSON.stringify(failure.payload)}\ncaused by: ${JSON.stringify(causes.map(({ payload }) => payload))}`);
    }
    return found !== undefined;
  });
  const reached = found!.sequence;
  const events = seen.filter((event) => event.sequence <= reached);
  cursor = reached;
  return events;
}

function approvalId(events: DurableEvent[]): string {
  const requested = events.at(-1);
  const id = requested?.payload.approvalId;
  assert.equal(typeof id, 'string', `approval.requested carries an id: ${JSON.stringify(requested)}`);
  return String(id);
}

/** What the sidecar logged about Boundary F calls for one operation. */
function boundaryCalls(operation: string, event = 'boundary_f'): Record<string, unknown>[] {
  const sidecarProcess = children.find(({ name }) => name === 'sidecar');
  return (sidecarProcess?.stderr ?? '')
    .split('\n')
    .filter((line) => line.startsWith('{'))
    .map((line): Record<string, unknown> => {
      const parsed: unknown = JSON.parse(line);
      return typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : {};
    })
    .filter((line) => line.event === event && line.operation === operation);
}

// --- the processes ---------------------------------------------------------------------------

function formaEnvironment(jwks: string, port: number): Record<string, string> {
  return {
    PATH: process.env.PATH ?? '',
    FORMA_BIND: `127.0.0.1:${port}`,
    FORMA_DATA_DIR: directory,
    FORMA_DATABASE_URL: `sqlite://${join(directory, 'forma.db')}`,
    FORMA_FONT_DIR: join(REPO, 'data/fonts'),
    FORMA_WEB_DIR: join(directory, 'no-web'),
    FORMA_AUTH_MODE: 'supabase',
    FORMA_SUPABASE_ISSUER: ISSUER,
    FORMA_SUPABASE_JWKS_URL: jwks,
    FORMA_LOG: 'warn',
  };
}

async function serveJwks(): Promise<string> {
  const server = createServer((_request, response) => {
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end(JWKS);
  });
  servers.push(server);
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  assert.ok(address && typeof address === 'object');
  return `http://127.0.0.1:${address.port}/jwks.json`;
}

async function freePort(): Promise<number> {
  const probe = createServer();
  probe.listen(0, '127.0.0.1');
  await once(probe, 'listening');
  const address = probe.address();
  assert.ok(address && typeof address === 'object');
  await new Promise((done) => probe.close(done));
  return address.port;
}

function run(binary: string, args: string[], env: Record<string, string>): string {
  const result = spawnSync(binary, args, { env, encoding: 'utf8' });
  assert.equal(result.status, 0, `${binary} ${args[0] ?? ''} failed: ${result.stderr}`);
  return result.stdout;
}

function start(name: string, binary: string, args: string[], env: Record<string, string>): Child {
  const child: Child = { process: spawn(binary, args, { env, stdio: ['pipe', 'pipe', 'pipe'] }), name, stdout: '', stderr: '' };
  child.process.stdout?.setEncoding('utf8').on('data', (chunk: string) => (child.stdout += chunk));
  child.process.stderr?.setEncoding('utf8').on('data', (chunk: string) => (child.stderr += chunk));
  children.push(child);
  return child;
}

/** The address a child prints on its `READY` line. */
async function ready(child: Child): Promise<string> {
  const stdout = child.process.stdout;
  assert.ok(stdout);
  const lines = createInterface({ input: stdout });
  try {
    return await new Promise<string>((done, fail) => {
      lines.on('line', (line) => {
        if (line.startsWith('READY ')) done(line.slice('READY '.length));
      });
      child.process.once('exit', (code) => fail(new Error(`${child.name} exited (${code}) before READY:\n${child.stderr}`)));
    });
  } finally {
    lines.close();
  }
}

async function stop(child: Child): Promise<void> {
  if (child.process.exitCode !== null || child.process.signalCode !== null) return;
  const exited = once(child.process, 'exit');
  child.process.kill('SIGTERM');
  const timer = setTimeout(() => child.process.kill('SIGKILL'), 5_000);
  await exited;
  clearTimeout(timer);
}

/** Polls `condition` until it holds. The deadline only turns a hang into a failure. */
async function until(what: string, condition: () => Promise<boolean>, watching?: Child): Promise<void> {
  const deadline = Date.now() + DEADLINE_MS;
  for (;;) {
    if (watching && watching.process.exitCode !== null) assert.fail(`${watching.name} exited while waiting for ${what}:\n${watching.stderr}`);
    if (await condition()) return;
    if (Date.now() > deadline) {
      assert.fail(`timed out waiting for ${what}\nevents: ${JSON.stringify(seen.slice(-12))}\n${children.map((child) => `--- ${child.name}\n${child.stderr.slice(-3000)}`).join('\n')}`);
    }
    await new Promise((done) => setTimeout(done, 25));
  }
}

/** A document of `pages` pages, the first page holding one text layer per id in `first`. */
function documentOf(pages: number, first: readonly string[]): unknown {
  const text = (id: string, line: number) => ({
    type: 'text',
    id,
    name: line === 0 ? 'Headline' : `Line ${line}`,
    transform: { x: 80, y: 160 + line * 260, width: 920, height: 220 },
    text: line === 0 ? 'HELLO' : 'A long line of\nset type to measure',
    font_family: 'Geist',
    font_size: 72,
    font_weight: 700,
    color: '#171717',
  });
  return {
    schema_version: 1,
    format: { preset: 'story', width: 1080, height: 1920 },
    pages: Array.from({ length: pages }, (_, index) => ({
      id: randomUUID(),
      name: `Page ${index + 1}`,
      layers: (index === 0 ? first : Array.from({ length: 6 }, () => randomUUID())).map(text),
    })),
  };
}
