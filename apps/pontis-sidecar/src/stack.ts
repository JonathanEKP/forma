/**
 * The processes of a local run, and the person who drives them (plan §11 steps 7 and 11).
 *
 * ```text
 * a test (the person) ── Forma session ──▶ sidecar ── /service/resolve ──▶ forma-server
 *                     ── intents ───────▶ sidecar ── device socket ──▶ test-driver
 *                                                                      (Pontis Runtime
 *                                                                       + a provider)
 *                     test-driver ── delegation ──▶ sidecar gateway ── Boundary F ──▶ forma-server
 * ```
 *
 * forma-server is the real binary in hosted mode, verifying session tokens
 * against a JWKS this module serves from `fixtures/identity`. The sidecar is the
 * real `src/main.ts` process. The driver is the Pontis Runtime linked from its
 * public crates (`test-driver/`); which provider it runs — FakeProvider or the
 * real Codex — is the caller's choice, and nothing else in the chain differs.
 * The person is played by the test: it starts the session, sends input, cancels
 * and answers approvals through the sidecar's routes, and reads what happened
 * from the durable event stream and from Forma.
 *
 * Everything here waits on conditions, never on a clock: every wait polls for an
 * event or a state, under a deadline that only turns a hang into a failure.
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

export const PACKAGE = resolve(import.meta.dirname, '..');
export const REPO = resolve(PACKAGE, '../..');
const FORMA_SERVER = process.env.FORMA_SERVER_BIN ?? join(REPO, 'target/debug/forma-server');
const DRIVER = process.env.FORMA_PONTIS_DRIVER_BIN ?? join(PACKAGE, 'test-driver/target/debug/forma-pontis-test-driver');

const ISSUER = 'https://project.supabase.co/auth/v1';
const SIGNING_KEY = readFileSync(join(REPO, 'fixtures/identity/test-signing-key.pkcs8.der'));
export const JWKS = readFileSync(join(REPO, 'fixtures/identity/test-jwks.json'));

export interface Child {
  readonly process: ChildProcess;
  readonly name: string;
  stdout: string;
  stderr: string;
}

export interface DurableEvent {
  readonly sequence: number;
  readonly type: string;
  readonly sessionId?: string;
  readonly payload: Record<string, unknown>;
}

export interface Design {
  readonly id: string;
  readonly version: number;
  readonly document: { readonly pages: readonly { readonly layers: readonly { readonly id: string; readonly font_size?: number }[] }[] };
}

export class Stack {
  readonly children: Child[] = [];
  /** Everything every event page has said so far, newest read. */
  seen: DurableEvent[] = [];
  /** The highest event sequence a wait has consumed. */
  cursor = 0;
  sessionId = '';
  directory = '';
  account = '';
  forma = '';
  sidecar = '';
  projectId = '';
  deadlineMs: number;

  private readonly servers: Server[] = [];
  readonly subject = randomUUID();
  /** The person's session, for a test that has to speak as their browser does. */
  readonly sessionToken = mintSession(this.subject);
  private credential = '';

  constructor(deadlineMs = 60_000) {
    this.deadlineMs = deadlineMs;
  }

  /** forma-server, an account and a project for the person, and the sidecar — no Runtime yet. */
  async boot(projectName: string, options: { editorOrigin?: string } = {}): Promise<void> {
    for (const [binary, how] of [
      [FORMA_SERVER, 'cargo build -p forma-server'],
      [DRIVER, 'cargo build --manifest-path apps/pontis-sidecar/test-driver/Cargo.toml'],
    ] as const) {
      assert.ok(spawnSync(binary, ['--help']).error === undefined, `${binary} is missing: ${how} (scripts/check.sh does this)`);
    }
    this.directory = await mkdtemp(join(tmpdir(), 'forma-pontis-'));
    const environment = this.formaEnvironment(await this.serveJwks(), await freePort());

    // An account for the person, and the sidecar's credential — both minted by
    // the operator commands a deployment uses.
    const granted = run(FORMA_SERVER, ['grant-owner', 'person@example.com', this.subject], environment);
    this.account = /^account (\S+)$/mu.exec(granted)?.[1] ?? assert.fail(`no account in ${granted}`);
    this.credential = run(FORMA_SERVER, ['service-credential', 'local-run'], environment).trim();
    assert.match(this.credential, /^forma_svc_/u);

    const server = this.start('forma-server', FORMA_SERVER, [], environment);
    this.forma = `http://${environment.FORMA_BIND}`;
    await this.until('forma-server to answer', async () => (await this.asPerson('GET', '/api/v1/status').catch(() => undefined))?.ok === true, server);
    this.projectId = ((await json(await this.asPerson('POST', '/api/v1/projects', { name: projectName }))) as { id: string }).id;

    const sidecarProcess = this.start('sidecar', process.execPath, [join(PACKAGE, 'src/main.ts')], {
      PATH: process.env.PATH ?? '',
      FORMA_SERVER_URL: this.forma,
      FORMA_SERVICE_CREDENTIAL: this.credential,
      FORMA_PONTIS_BIND: '127.0.0.1:0',
      // Without it no window can connect and every ui.* capability answers ui_unavailable.
      ...(options.editorOrigin === undefined ? {} : { FORMA_PONTIS_EDITOR_ORIGIN: options.editorOrigin }),
    });
    this.sidecar = await ready(sidecarProcess);
  }

  /**
   * Pairs the driver as a Device and, given a `providerId`, opens a session and waits for it to start.
   * `driverEnvironment` is what that provider needs (`FORMA_DRIVER_*`).
   */
  async attachDriver(driverEnvironment: Record<string, string>, providerId?: string): Promise<void> {
    const pairing = (await json(await this.toSidecar('POST', '/pons/v1/pairing/codes', {}))) as { code: string; applicationId: string };
    const driver = this.start('test-driver', DRIVER, [], {
      PATH: process.env.PATH ?? '',
      FORMA_PONTIS_ORIGIN: this.sidecar,
      FORMA_PONTIS_APPLICATION_ID: pairing.applicationId,
      FORMA_DRIVER_PAIRING_CODE: pairing.code,
      ...driverEnvironment,
    });
    await ready(driver);

    if (providerId !== undefined) await this.startSession(providerId);
  }

  /** Opens a session with `providerId` through the sidecar, and waits for it to start. */
  async startSession(providerId: string): Promise<void> {
    const started = (await json(await this.toSidecar('POST', '/agent/intent', { type: 'session.start', payload: { providerId } }))) as {
      sessionId: string;
      delivered: boolean;
    };
    assert.equal(started.delivered, true);
    this.sessionId = started.sessionId;
    await this.nextEvent('session.started');
  }

  /** Stops every process and removes the data. The two credentials never reach any output. */
  async close(): Promise<void> {
    await Promise.allSettled(this.children.map(stop));
    await Promise.allSettled(this.servers.map((server) => new Promise((done) => server.close(done))));
    if (this.directory) await rm(this.directory, { recursive: true, force: true });
    const output = JSON.stringify(this.children.map(({ stdout, stderr }) => [stdout, stderr]));
    // Checked last, over everything every process printed.
    assert.ok(!output.includes(this.credential), 'the service credential never reaches any output');
    assert.ok(!output.includes(this.sessionToken), 'the session token never reaches any output');
  }

  // --- the person ----------------------------------------------------------------------------

  async asPerson(method: string, path: string, body?: unknown): Promise<Response> {
    return await fetch(`${this.forma}${path}`, this.request(method, body));
  }

  async toSidecar(method: string, path: string, body?: unknown): Promise<Response> {
    return await fetch(`${this.sidecar}${path}`, this.request(method, body));
  }

  private request(method: string, body?: unknown): RequestInit {
    return {
      method,
      headers: { authorization: `Bearer ${this.sessionToken}`, 'content-type': 'application/json' },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    };
  }

  async read(id: string): Promise<Design> {
    return (await json(await this.asPerson('GET', `/api/v1/designs/${id}`))) as Design;
  }

  async createDesign(name: string, document: unknown): Promise<Design> {
    return (await json(await this.asPerson('POST', `/api/v1/projects/${this.projectId}/designs`, { name, format: null, document }))) as Design;
  }

  async intent(type: string, payload: Record<string, unknown>): Promise<void> {
    const answer = (await json(await this.toSidecar('POST', '/agent/intent', { type, payload }))) as { delivered: boolean };
    assert.equal(answer.delivered, true, `${type} reached the Runtime`);
  }

  async say(text: string, view?: Record<string, unknown>): Promise<void> {
    await this.intent('session.input', { sessionId: this.sessionId, text, ...(view === undefined ? {} : { view }) });
  }

  /**
   * The view lines the driver took off the messages it was given, in order (`VIEW <line>` on its
   * stderr): what the sidecar put in front of the person's text, as the Runtime received it.
   */
  viewLines(): string[] {
    const driver = this.children.find(({ name }) => name === 'test-driver');
    return (driver?.stderr ?? '')
      .split('\n')
      .filter((line) => line.startsWith('VIEW '))
      .map((line) => line.slice('VIEW '.length));
  }

  /** Sends `text` and returns every event of the turn, up to `terminal`. */
  async turn(text: string, terminal: string): Promise<DurableEvent[]> {
    const sent = this.cursor;
    await this.say(text);
    const events = await this.nextEvent(terminal);
    return events.filter((event) => event.sequence > sent);
  }

  /** Reads the durable event page into `seen`, and returns it. */
  async refresh(): Promise<readonly DurableEvent[]> {
    const page = (await json(
      await this.toSidecar('GET', `/pons/v1/events?scope=application_context&applicationContextId=${this.account}&after=0`),
    )) as { events: DurableEvent[] };
    this.seen = page.events;
    return this.seen;
  }

  /** Waits for the next event of `type` past the cursor; returns everything read up to it. */
  async nextEvent(type: string, matches: (event: DurableEvent) => boolean = () => true): Promise<DurableEvent[]> {
    let found: DurableEvent | undefined;
    await this.until(`${type}`, async () => {
      await this.refresh();
      found = this.seen.find(
        (event) => event.sequence > this.cursor && event.type === type && (event.sessionId ?? this.sessionId) === this.sessionId && matches(event),
      );
      for (const failure of this.seen.filter((event) => event.sequence > this.cursor && (event.type === 'turn.failed' || event.type === 'session.failed'))) {
        const causes = this.seen.filter((event) => event.sequence > this.cursor && event.type === 'capability.failed');
        assert.fail(`${failure.type}: ${JSON.stringify(failure.payload)}\ncaused by: ${JSON.stringify(causes.map(({ payload }) => payload))}`);
      }
      return found !== undefined;
    });
    const reached = found!.sequence;
    const events = this.seen.filter((event) => event.sequence <= reached);
    this.cursor = reached;
    return events;
  }

  /** What the sidecar logged about Boundary F calls for one operation. */
  boundaryCalls(operation: string, event = 'boundary_f'): Record<string, unknown>[] {
    const sidecarProcess = this.children.find(({ name }) => name === 'sidecar');
    return (sidecarProcess?.stderr ?? '')
      .split('\n')
      .filter((line) => line.startsWith('{'))
      .map((line): Record<string, unknown> => {
        const parsed: unknown = JSON.parse(line);
        return typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : {};
      })
      .filter((line) => line.event === event && line.operation === operation);
  }

  // --- the processes -------------------------------------------------------------------------

  private formaEnvironment(jwks: string, port: number): Record<string, string> {
    return {
      PATH: process.env.PATH ?? '',
      FORMA_BIND: `127.0.0.1:${port}`,
      FORMA_DATA_DIR: this.directory,
      FORMA_DATABASE_URL: `sqlite://${join(this.directory, 'forma.db')}`,
      FORMA_FONT_DIR: join(REPO, 'data/fonts'),
      FORMA_WEB_DIR: join(this.directory, 'no-web'),
      FORMA_AUTH_MODE: 'supabase',
      FORMA_SUPABASE_ISSUER: ISSUER,
      FORMA_SUPABASE_JWKS_URL: jwks,
      FORMA_LOG: 'warn',
    };
  }

  private async serveJwks(): Promise<string> {
    const server = createServer((_request, response) => {
      response.writeHead(200, { 'content-type': 'application/json' });
      response.end(JWKS);
    });
    this.servers.push(server);
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    const address = server.address();
    assert.ok(address && typeof address === 'object');
    return `http://127.0.0.1:${address.port}/jwks.json`;
  }

  start(name: string, binary: string, args: string[], env: Record<string, string>): Child {
    const child: Child = { process: spawn(binary, args, { env, stdio: ['pipe', 'pipe', 'pipe'] }), name, stdout: '', stderr: '' };
    child.process.stdout?.setEncoding('utf8').on('data', (chunk: string) => (child.stdout += chunk));
    child.process.stderr?.setEncoding('utf8').on('data', (chunk: string) => (child.stderr += chunk));
    this.children.push(child);
    return child;
  }

  /** Polls `condition` until it holds. The deadline only turns a hang into a failure. */
  async until(what: string, condition: () => Promise<boolean>, watching?: Child): Promise<void> {
    const deadline = Date.now() + this.deadlineMs;
    for (;;) {
      if (watching && watching.process.exitCode !== null) assert.fail(`${watching.name} exited while waiting for ${what}:\n${watching.stderr}`);
      if (await condition()) return;
      if (Date.now() > deadline) {
        assert.fail(
          `timed out waiting for ${what}\nevents: ${JSON.stringify(this.seen.slice(-12))}\n${this.children
            .map((child) => `--- ${child.name}\n${child.stderr.slice(-3000)}`)
            .join('\n')}`,
        );
      }
      await new Promise((done) => setTimeout(done, 25));
    }
  }
}

// --- the person's session ------------------------------------------------------------------

export interface SessionOptions {
  readonly issuer?: string;
  readonly email?: string;
  /** How long the token lives. An hour is plenty for a test; a person at a browser needs longer. */
  readonly ttlSeconds?: number;
}

/** A Supabase-shaped access token for `subject`, signed with the fixture key (`fixtures/identity`). */
export function mintSession(subject: string, options: SessionOptions = {}): string {
  const encode = (value: unknown) => Buffer.from(JSON.stringify(value)).toString('base64url');
  const body = `${encode({ alg: 'ES256', typ: 'JWT', kid: 'test-key-1' })}.${encode({
    sub: subject,
    iss: options.issuer ?? ISSUER,
    aud: 'authenticated',
    exp: Math.floor(Date.now() / 1000) + (options.ttlSeconds ?? 3600),
    email: options.email ?? 'person@example.com',
    email_verified: true,
  })}`;
  const key = createPrivateKey({ key: SIGNING_KEY, format: 'der', type: 'pkcs8' });
  return `${body}.${sign('sha256', Buffer.from(body), { key, dsaEncoding: 'ieee-p1363' }).toString('base64url')}`;
}

export async function json(response: Response): Promise<unknown> {
  const text = await response.text();
  assert.ok(response.ok, `${response.status} ${response.url}: ${text}`);
  const parsed: unknown = JSON.parse(text);
  return parsed;
}

export function approvalId(events: DurableEvent[]): string {
  const requested = events.at(-1);
  const id = requested?.payload.approvalId;
  assert.equal(typeof id, 'string', `approval.requested carries an id: ${JSON.stringify(requested)}`);
  return String(id);
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

export function run(binary: string, args: string[], env: Record<string, string>): string {
  const result = spawnSync(binary, args, { env, encoding: 'utf8' });
  assert.equal(result.status, 0, `${binary} ${args[0] ?? ''} failed: ${result.stderr}`);
  return result.stdout;
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

/** A document of `pages` pages, the first page holding one text layer per id in `first`. */
export function documentOf(pages: number, first: readonly string[]): unknown {
  const text = (id: string, line: number) => ({
    type: 'text',
    id,
    name: line === 0 ? 'Headline' : line === 1 ? 'Tagline' : `Line ${line}`,
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
