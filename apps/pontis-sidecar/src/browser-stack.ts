/**
 * An isolated instance of the whole Pontis → Forma chain, for a person at a real browser
 * (`docs/pontis-browser-testing.md`).
 *
 * ```text
 * browser ── Vite (apps/web) ─┬─ /api ──────────▶ forma-server  (hosted mode, fixture identity)
 *    │                        └─ /pons, /agent ─▶ sidecar ── device socket ──▶ test-driver
 *    │                                                                         (FakeProvider or the
 *    └─ supabase-js ── session refresh ──▶ fake identity provider               real Codex)
 *                                           (the fixture JWKS, and nothing else)
 * ```
 *
 * `stack.ts` is the same chain for a test, where the person is code. Here the person is at the
 * editor, so what this adds is everything a browser needs and a test does not: a fake identity
 * provider the editor can be pointed at (so nothing ever contacts a real one), a Vite dev server,
 * two people in two accounts with something to act on, a snippet that signs the browser in, and
 * Runtimes paired to those people. Every instance's ports are a function of its number, so several
 * run at once without colliding.
 *
 * Commands: `start`, `stop`, `signin`, `setup-codex`; see `usage`.
 */
import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { once } from 'node:events';
import { createWriteStream, existsSync, lstatSync, mkdirSync, readFileSync, readlinkSync, realpathSync, rmSync, symlinkSync, writeFileSync, type WriteStream } from 'node:fs';
import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { createInterface } from 'node:readline';

import { documentOf, JWKS, mintSession, PACKAGE, REPO, run } from './stack.ts';

const FORMA_SERVER = process.env.FORMA_SERVER_BIN ?? join(REPO, 'target/debug/forma-server');
const DRIVER = process.env.FORMA_PONTIS_DRIVER_BIN ?? join(PACKAGE, 'test-driver/target/debug/forma-pontis-test-driver');
const BRIDGE = process.env.FORMA_DRIVER_MCP_BRIDGE ?? join(PACKAGE, 'test-driver/target/debug/pontis-mcp-bridge');
const VITE = join(REPO, 'apps/web/node_modules/vite/bin/vite.js');

/** The Codex release Pontis accepts for `capability_only` (docs/pontis-step11-real-codex.md). */
const CODEX_VERSION = '0.158.0';
const CODEX_CONFIG = 'model = "gpt-6-luna"\nmodel_reasoning_effort = "low"\n';

/** Ports are `BASE + 10 * N + offset`, so instance N owns `BASE + 10N … BASE + 10N + 9`. */
const BASE_PORT = 4800;
/** Long enough for a tester's whole session; supabase-js refreshes against the fake provider after that. */
const SESSION_SECONDS = 6 * 60 * 60;
const STARTUP_DEADLINE_MS = 120_000;

type PersonKey = 'a' | 'b';
type ProviderName = 'fake' | 'codex';

interface Seeded {
  readonly email: string;
  readonly subject: string;
  readonly accountId: string;
  readonly projectId: string;
  readonly designs: readonly { readonly id: string; readonly name: string }[];
}

/** What `stop` and `signin` need from a running instance. Contains no credential. */
interface State {
  readonly instance: number;
  readonly directory: string;
  readonly pid: number;
  readonly processes: readonly { readonly name: string; readonly pid: number; readonly log: string }[];
  readonly urls: { readonly editor: string; readonly editorAlias: string; readonly forma: string; readonly sidecar: string; readonly idp: string };
  readonly ports: Record<string, number>;
  readonly people: Record<PersonKey, Seeded>;
  readonly provider: Record<PersonKey, ProviderName | null>;
}

// --- arguments ----------------------------------------------------------------------------------

function usage(): never {
  process.stderr.write(`usage:
  browser-stack start <N> [--provider fake|codex] [--provider-a …] [--provider-b …] [--devices a|b|a,b|none]
  browser-stack signin <N> <a|b>          print the snippet that signs the browser in (fresh token)
  browser-stack stop <N>                  stop an instance started elsewhere
  browser-stack setup-codex               install the pinned Codex 0.158.0 and its CODEX_HOME (idempotent)

  N is the instance number, 0 to 99. Ports: forma-server 4800+10N, sidecar +1, editor +2, identity +3.
`);
  process.exit(2);
}

function parseInstance(raw: string | undefined): number {
  const n = Number(raw);
  if (!Number.isInteger(n) || n < 0 || n > 99) usage();
  return n;
}

interface Options {
  readonly providers: Record<PersonKey, ProviderName>;
  readonly devices: readonly PersonKey[];
}

function parseOptions(args: readonly string[]): Options {
  const providers: Record<PersonKey, ProviderName> = { a: 'fake', b: 'fake' };
  let devices: readonly PersonKey[] = ['a'];
  const provider = (value: string | undefined): ProviderName => (value === 'fake' || value === 'codex' ? value : usage());
  for (let index = 0; index < args.length; index += 1) {
    const flag = args[index];
    const value = args[index + 1];
    index += 1;
    if (flag === '--provider') {
      providers.a = provider(value);
      providers.b = providers.a;
    } else if (flag === '--provider-a') providers.a = provider(value);
    else if (flag === '--provider-b') providers.b = provider(value);
    else if (flag === '--devices') {
      if (value === 'none') devices = [];
      else if (value === 'a' || value === 'b' || value === 'a,b' || value === 'b,a') devices = value.split(',').filter((k): k is PersonKey => k === 'a' || k === 'b');
      else usage();
    } else usage();
  }
  return { providers, devices };
}

const instanceDirectory = (n: number) => join(tmpdir(), 'forma-browser-stack', String(n));
const portsOf = (n: number) => ({ forma: BASE_PORT + 10 * n, sidecar: BASE_PORT + 10 * n + 1, editor: BASE_PORT + 10 * n + 2, idp: BASE_PORT + 10 * n + 3 });

// --- small helpers --------------------------------------------------------------------------------

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function fail(message: string): never {
  throw new Error(message);
}

async function portIsFree(port: number): Promise<boolean> {
  const probe = createServer();
  probe.listen(port, '127.0.0.1');
  try {
    await once(probe, 'listening');
  } catch {
    return false;
  }
  await new Promise((done) => probe.close(done));
  return true;
}

async function until(what: string, condition: () => Promise<boolean>, watching?: Proc): Promise<void> {
  const deadline = Date.now() + STARTUP_DEADLINE_MS;
  for (;;) {
    if (watching && watching.child.exitCode !== null) fail(`${watching.name} exited while waiting for ${what}; see ${watching.log}`);
    if (await condition().catch(() => false)) return;
    if (Date.now() > deadline) fail(`timed out waiting for ${what}`);
    await new Promise((done) => setTimeout(done, 100));
  }
}

// --- processes ------------------------------------------------------------------------------------

/** A child in its own process group, writing to its own log file; the group is what is stopped. */
interface Proc {
  readonly name: string;
  readonly child: ChildProcess;
  readonly log: string;
  /** The address of its `READY <address>` line, once printed. */
  readonly ready: Promise<string>;
}

function launch(name: string, directory: string, command: string, args: readonly string[], env: Record<string, string>, cwd?: string): Proc {
  mkdirSync(join(directory, 'logs'), { recursive: true });
  const log = join(directory, 'logs', `${name}.log`);
  const sink: WriteStream = createWriteStream(log, { flags: 'a' });
  const child = spawn(command, args, { env, cwd, detached: true, stdio: ['pipe', 'pipe', 'pipe'] });
  child.stderr.on('data', (chunk: Buffer) => sink.write(chunk));
  const lines = createInterface({ input: child.stdout });
  const ready = new Promise<string>((done, failed) => {
    lines.on('line', (line) => {
      sink.write(`${line}\n`);
      if (line.startsWith('READY ')) done(line.slice('READY '.length));
    });
    child.once('exit', (code) => failed(new Error(`${name} exited (${String(code)}) before READY; see ${log}`)));
  });
  // A rejection nobody awaits (a process that never needed READY) must not take the harness down.
  ready.catch(() => undefined);
  return { name, child, log, ready };
}

async function stopProc(proc: Proc): Promise<void> {
  const { child } = proc;
  if (child.exitCode !== null || child.signalCode !== null || child.pid === undefined) return;
  const exited = once(child, 'exit');
  const group = -child.pid;
  try {
    process.kill(group, 'SIGTERM');
  } catch {
    return;
  }
  const timer = setTimeout(() => {
    try {
      process.kill(group, 'SIGKILL');
    } catch {
      // already gone
    }
  }, 5_000);
  await exited;
  clearTimeout(timer);
}

// --- the fake identity provider -------------------------------------------------------------------

/**
 * What the editor's supabase-js is pointed at in place of a real provider.
 *
 * It publishes the fixture JWKS (which forma-server verifies against) and answers the three calls
 * supabase-js makes on its own — refresh, user, logout — so a session that outlives its token is
 * renewed instead of ending in a sign-out. It never issues a session to anyone who has not already
 * presented a refresh token this harness minted: there is no sign-in route at all.
 */
async function startIdentity(port: number, issuer: string, origins: readonly string[], people: ReadonlyMap<string, { email: string }>, log: WriteStream): Promise<Server> {
  const handle = async (request: IncomingMessage, response: ServerResponse): Promise<void> => {
    const url = new URL(request.url ?? '/', `http://127.0.0.1:${port}`);
    log.write(`${new Date().toISOString()} ${request.method ?? '?'} ${url.pathname}\n`);
    const origin = request.headers.origin;
    const headers: Record<string, string> = { 'content-type': 'application/json' };
    if (origin !== undefined && origins.includes(origin)) {
      headers['access-control-allow-origin'] = origin;
      headers['access-control-allow-headers'] = request.headers['access-control-request-headers'] ?? '*';
      headers['access-control-allow-methods'] = 'GET, POST, PUT, DELETE, OPTIONS';
      headers.vary = 'origin';
    }
    const reply = (status: number, body?: unknown) => {
      response.writeHead(status, headers);
      response.end(body === undefined ? undefined : JSON.stringify(body));
    };
    if (request.method === 'OPTIONS') return reply(204);
    if (url.pathname === '/auth/v1/.well-known/jwks.json' || url.pathname === '/jwks.json') {
      response.writeHead(200, headers);
      response.end(JWKS);
      return;
    }
    if (url.pathname === '/auth/v1/logout') return reply(204);
    // The Agents page lists the OAuth applications a person authorized; none exist here.
    if (url.pathname === '/auth/v1/user/oauth/grants' && request.method === 'GET') return reply(200, []);
    if (url.pathname === '/auth/v1/user' && request.method === 'GET') {
      const subject = subjectOfBearer(request.headers.authorization);
      const person = subject === undefined ? undefined : people.get(subject);
      return subject === undefined || person === undefined ? reply(401, { msg: 'no session' }) : reply(200, userOf(subject, person.email));
    }
    if (url.pathname === '/auth/v1/token' && url.searchParams.get('grant_type') === 'refresh_token') {
      let raw = '';
      for await (const chunk of request) raw += String(chunk);
      const parsed: unknown = JSON.parse(raw || '{}');
      const token = isRecord(parsed) && typeof parsed.refresh_token === 'string' ? parsed.refresh_token : '';
      const subject = token.startsWith(REFRESH_PREFIX) ? token.slice(REFRESH_PREFIX.length) : '';
      const person = people.get(subject);
      return person === undefined ? reply(400, { error: 'invalid_grant', error_description: 'unknown refresh token' }) : reply(200, sessionOf(subject, person.email, issuer));
    }
    reply(404, { msg: `the fake identity provider has no ${url.pathname}` });
  };
  const server = createServer((request, response) => {
    handle(request, response).catch(() => {
      response.writeHead(500).end();
    });
  });
  // Listening before anything asks for the keys: forma-server fetches them once, at startup, and a
  // refused connection there is a refusal to start.
  server.listen(port, '127.0.0.1');
  await once(server, 'listening');
  return server;
}

const REFRESH_PREFIX = 'fake-refresh.';

function subjectOfBearer(header: string | undefined): string | undefined {
  const token = header?.startsWith('Bearer ') ? header.slice('Bearer '.length) : undefined;
  const payload = token?.split('.')[1];
  if (payload === undefined) return undefined;
  try {
    const parsed: unknown = JSON.parse(Buffer.from(payload, 'base64url').toString('utf8'));
    return isRecord(parsed) && typeof parsed.sub === 'string' ? parsed.sub : undefined;
  } catch {
    return undefined;
  }
}

function userOf(subject: string, email: string): Record<string, unknown> {
  return {
    id: subject,
    aud: 'authenticated',
    role: 'authenticated',
    email,
    email_confirmed_at: '2026-01-01T00:00:00Z',
    app_metadata: { provider: 'email', providers: ['email'] },
    user_metadata: {},
    created_at: '2026-01-01T00:00:00Z',
  };
}

/** The object supabase-js keeps in localStorage, and what its refresh call returns. */
function sessionOf(subject: string, email: string, issuer: string): Record<string, unknown> {
  return {
    access_token: mintSession(subject, { issuer, email, ttlSeconds: SESSION_SECONDS }),
    token_type: 'bearer',
    expires_in: SESSION_SECONDS,
    expires_at: Math.floor(Date.now() / 1000) + SESSION_SECONDS,
    refresh_token: `${REFRESH_PREFIX}${subject}`,
    user: userOf(subject, email),
  };
}

/** supabase-js's key: `sb-<first label of the project host>-auth-token`. */
function storageKey(idp: string): string {
  return `sb-${new URL(idp).hostname.split('.')[0] ?? 'localhost'}-auth-token`;
}

function signinSnippet(state: Pick<State, 'urls' | 'people'>, who: PersonKey): string {
  const person = state.people[who];
  const session = sessionOf(person.subject, person.email, `${state.urls.idp}/auth/v1`);
  return `(() => { localStorage.setItem(${JSON.stringify(storageKey(state.urls.idp))}, ${JSON.stringify(JSON.stringify(session))}); setTimeout(() => location.reload(), 50); return ${JSON.stringify(`signed in as ${person.email}`)}; })()`;
}

// --- seeding --------------------------------------------------------------------------------------

type Layer = Record<string, unknown>;

const text = (name: string, content: string, y: number, size: number, weight = 700, color = '#171717'): Layer => ({
  type: 'text',
  id: randomUUID(),
  name,
  transform: { x: 80, y, width: 920, height: Math.round(size * 1.4) * (content.split('\n').length) },
  text: content,
  font_family: 'Geist',
  font_size: size,
  font_weight: weight,
  color,
});

const shape = (name: string, kind: 'rectangle' | 'ellipse', x: number, y: number, width: number, height: number, fill: string): Layer => ({
  type: 'shape',
  id: randomUUID(),
  name,
  transform: { x, y, width, height },
  kind,
  fill,
});

function page(name: string, layers: readonly Layer[]): Record<string, unknown> {
  return { id: randomUUID(), name, background: '#F4F1EA', layers };
}

function design(preset: 'story' | 'square', pages: readonly Record<string, unknown>[]): unknown {
  const size = preset === 'story' ? { width: 1080, height: 1920 } : { width: 1080, height: 1080 };
  return { schema_version: 1, format: { preset, ...size }, pages };
}

/** Two designs for `tag`, whose texts say whose they are so a leak across accounts is visible on sight. */
function documentsFor(tag: string, accent: string): readonly { readonly name: string; readonly document: unknown }[] {
  return [
    {
      name: `${tag} Launch cover`,
      document: design('story', [
        page('Cover', [
          shape('Accent block', 'rectangle', 80, 1500, 920, 220, accent),
          text('Headline', `${tag}\nLAUNCH`, 220, 160),
          text('Tagline', `A tagline that belongs to ${tag}`, 760, 56, 400),
          text('Footer', `${tag.toLowerCase()}.example`, 1560, 48, 600, '#171717'),
        ]),
      ]),
    },
    {
      name: `${tag} Square post`,
      document: design('square', [
        page('Post', [
          shape('Circle', 'ellipse', 640, 120, 360, 360, accent),
          text('Title', `${tag} NOTES`, 140, 96),
          text('Body', `Three short lines\nwritten for ${tag}\nand for nobody else`, 560, 56, 400),
        ]),
      ]),
    },
  ];
}

class Api {
  private readonly forma: string;
  private readonly token: string;

  constructor(forma: string, token: string) {
    this.forma = forma;
    this.token = token;
  }

  async call(method: string, path: string, body?: unknown): Promise<unknown> {
    const response = await fetch(`${this.forma}${path}`, {
      method,
      headers: { authorization: `Bearer ${this.token}`, 'content-type': 'application/json' },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    const raw = await response.text();
    if (!response.ok) fail(`${method} ${path}: ${response.status} ${raw.slice(0, 300)}`);
    const parsed: unknown = JSON.parse(raw);
    return parsed;
  }

  async id(method: string, path: string, body?: unknown): Promise<string> {
    const answer = await this.call(method, path, body);
    return isRecord(answer) && typeof answer.id === 'string' ? answer.id : fail(`${method} ${path}: no id in the answer`);
  }
}

// --- the instance ---------------------------------------------------------------------------------

async function start(n: number, options: Options): Promise<void> {
  const ports = portsOf(n);
  for (const [what, port] of Object.entries(ports)) {
    if (!(await portIsFree(port))) fail(`instance ${n}: port ${port} (${what}) is in use — is it already running? \`stop ${n}\`, or pick another number`);
  }
  for (const [what, path] of [
    ['forma-server', FORMA_SERVER],
    ['the test driver', DRIVER],
    ['vite', VITE],
  ] as const) {
    if (!existsSync(path)) fail(`${what} is missing at ${path}: cargo build -p forma-server; cargo build --manifest-path apps/pontis-sidecar/test-driver/Cargo.toml; pnpm install`);
  }
  const codexWanted = options.devices.some((who) => options.providers[who] === 'codex');
  const codex = codexWanted ? checkCodex() : undefined;

  const directory = instanceDirectory(n);
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(join(directory, 'logs'), { recursive: true });
  mkdirSync(join(directory, 'forma'), { recursive: true });

  const procs: Proc[] = [];
  const servers: Server[] = [];
  const logs: WriteStream[] = [];
  let stopping = false;
  const stopAll = async (): Promise<void> => {
    if (stopping) return;
    stopping = true;
    for (const proc of [...procs].reverse()) await stopProc(proc);
    await Promise.allSettled(servers.map((server) => new Promise((done) => server.close(done))));
    for (const log of logs) log.end();
    rmSync(join(directory, 'state.json'), { force: true });
  };
  const onSignal = () => {
    process.stdout.write('\nstopping instance…\n');
    void stopAll().then(() => process.exit(0));
  };
  process.on('SIGINT', onSignal);
  process.on('SIGTERM', onSignal);

  try {
    const idp = `http://127.0.0.1:${ports.idp}`;
    const issuer = `${idp}/auth/v1`;
    const editor = `http://127.0.0.1:${ports.editor}`;
    // The same server under a second name: one browser origin holds one session, and `localhost` is
    // the cheapest second origin, so person A and person B can be signed in side by side.
    const editorAlias = `http://localhost:${ports.editor}`;
    const forma = `http://127.0.0.1:${ports.forma}`;
    // Stable per instance number, so a browser's stored session stays valid across a restart of the
    // same instance (the data is wiped, the identities are not).
    const subjects = { a: subjectOf(n, 'a'), b: subjectOf(n, 'b') };
    const emails = { a: `person-a-${n}@example.test`, b: `person-b-${n}@example.test` };
    const known = new Map<string, { email: string }>([
      [subjects.a, { email: emails.a }],
      [subjects.b, { email: emails.b }],
    ]);

    const idpLog = createWriteStream(join(directory, 'logs', 'identity.log'), { flags: 'a' });
    logs.push(idpLog);
    servers.push(await startIdentity(ports.idp, issuer, [editor, editorAlias], known, idpLog));

    const base = { PATH: process.env.PATH ?? '', HOME: process.env.HOME ?? '' };
    const formaEnv: Record<string, string> = {
      PATH: base.PATH,
      FORMA_BIND: `127.0.0.1:${ports.forma}`,
      FORMA_DATA_DIR: join(directory, 'forma'),
      FORMA_DATABASE_URL: `sqlite://${join(directory, 'forma', 'forma.db')}`,
      FORMA_FONT_DIR: join(REPO, 'data/fonts'),
      FORMA_WEB_DIR: join(directory, 'no-web'),
      FORMA_AUTH_MODE: 'supabase',
      FORMA_SUPABASE_ISSUER: issuer,
      FORMA_SUPABASE_JWKS_URL: `${idp}/auth/v1/.well-known/jwks.json`,
      FORMA_LOG: 'info',
    };
    // Person A owns the bootstrap account; person B is invited and creates their own, so the two
    // are in different accounts (the only way to have two: `grant-owner` always joins the first).
    const granted = run(FORMA_SERVER, ['grant-owner', emails.a, subjects.a], formaEnv);
    const accountA = /^account (\S+)$/mu.exec(granted)?.[1] ?? fail(`no account in ${granted}`);
    run(FORMA_SERVER, ['invite', emails.b], formaEnv);
    const credential = run(FORMA_SERVER, ['service-credential', `browser-stack-${n}`], formaEnv).trim();
    if (!credential.startsWith('forma_svc_')) fail('forma-server minted no service credential');

    const server = launch('forma-server', directory, FORMA_SERVER, [], formaEnv);
    procs.push(server);
    await until('forma-server', async () => (await fetch(`${forma}/api/v1/identity`)).ok, server);

    const token = (who: PersonKey) => mintSession(subjects[who], { issuer, email: emails[who], ttlSeconds: SESSION_SECONDS });
    const api = { a: new Api(forma, token('a')), b: new Api(forma, token('b')) };
    const accountB = await api.b.id('POST', '/api/v1/accounts');
    const seeded: Record<PersonKey, Seeded> = { a: seededStub(emails.a, subjects.a, accountA), b: seededStub(emails.b, subjects.b, accountB) };
    const fakeScripts = new Map<PersonKey, Record<string, string>>();
    for (const who of ['a', 'b'] as const) {
      const tag = who === 'a' ? 'ALPHA' : 'BRAVO';
      const projectId = await api[who].id('POST', '/api/v1/projects', { name: `${tag} project` });
      await api[who].call('PUT', `/api/v1/projects/${projectId}/brand`, brandFor(tag, who === 'a' ? '#FF5A36' : '#2F6BFF'));
      const made: { id: string; name: string }[] = [];
      let first: unknown;
      for (const { name, document } of documentsFor(tag, who === 'a' ? '#FF5A36' : '#2F6BFF')) {
        const created = await api[who].call('POST', `/api/v1/projects/${projectId}/designs`, { name, format: null, document });
        const id = isRecord(created) && typeof created.id === 'string' ? created.id : fail('no design id');
        made.push({ id, name });
        first ??= created;
      }
      // The FakeProvider scripts name real ids; a real model reads them from the design.
      if (options.providers[who] === 'fake') {
        const longName = `${tag} Long (120 pages)`;
        made.push({ id: await api[who].id('POST', `/api/v1/projects/${projectId}/designs`, { name: longName, format: null, document: documentOf(120, [randomUUID()]) }), name: longName });
        fakeScripts.set(who, fakeEnvironment(projectId, made, first));
      }
      seeded[who] = { ...seeded[who], projectId, designs: made };
    }

    const sidecar = launch('sidecar', directory, process.execPath, [join(PACKAGE, 'src/main.ts')], {
      PATH: base.PATH,
      FORMA_SERVER_URL: forma,
      FORMA_SERVICE_CREDENTIAL: credential,
      FORMA_PONTIS_BIND: `127.0.0.1:${ports.sidecar}`,
      FORMA_PONTIS_EDITOR_ORIGIN: `${editor},${editorAlias}`,
      // Inside the instance's directory, so it is wiped with the rest on the next start of this number
      // and a restart of the sidecar alone (same variable) keeps devices and history.
      FORMA_PONTIS_DATA_DIR: join(directory, 'pontis'),
    });
    procs.push(sidecar);
    const sidecarUrl = await sidecar.ready;

    // The identity variables come from here and never from apps/web/.env, which points at a real
    // project: a process variable outranks an .env file in Vite, and the check below proves it
    // rather than assuming it.
    const vite = launch(
      'vite',
      directory,
      process.execPath,
      [VITE, '--host', '127.0.0.1', '--port', String(ports.editor), '--strictPort'],
      {
        ...base,
        VITE_SUPABASE_URL: idp,
        VITE_SUPABASE_PUBLISHABLE_KEY: 'fake-publishable-key-for-the-browser-stack',
        FORMA_SERVER_URL: forma,
        FORMA_PONTIS_URL: sidecarUrl,
      },
      join(REPO, 'apps/web'),
    );
    procs.push(vite);
    await until('vite', async () => (await fetch(editor)).ok, vite);
    await assertFakeIdentity(editor, idp);
    await assertProxies(editor);

    const provider: Record<PersonKey, ProviderName | null> = { a: null, b: null };
    for (const who of options.devices) {
      const chosen = options.providers[who];
      const pairing = await pairingCode(sidecarUrl, token(who));
      const driver = launch(`driver-${who}`, directory, DRIVER, [], {
        PATH: base.PATH,
        HOME: base.HOME,
        FORMA_PONTIS_ORIGIN: sidecarUrl,
        FORMA_PONTIS_APPLICATION_ID: pairing.applicationId,
        FORMA_DRIVER_PAIRING_CODE: pairing.code,
        ...(chosen === 'codex' && codex !== undefined
          ? { FORMA_DRIVER_PROVIDER: 'codex', FORMA_DRIVER_CODEX_BIN: codex.bin, FORMA_DRIVER_CODEX_HOME: codex.home, FORMA_DRIVER_MCP_BRIDGE: BRIDGE }
          : fakeScripts.get(who) ?? {}),
      });
      procs.push(driver);
      await driver.ready;
      provider[who] = chosen;
      if (chosen === 'codex' && codex?.cold === true) {
        process.stdout.write(`first run on a new CODEX_HOME: warming it with one session (free, sends no prompt)…\n`);
        await warmCodex(sidecarUrl, token(who));
      }
    }

    const state: State = {
      instance: n,
      directory,
      pid: process.pid,
      processes: procs.map(({ name, child, log }) => ({ name, pid: child.pid ?? 0, log })),
      urls: { editor, editorAlias, forma, sidecar: sidecarUrl, idp },
      ports: { ...ports },
      people: seeded,
      provider,
    };
    for (const who of ['a', 'b'] as const) writeFileSync(join(directory, `signin-${who}.js`), `${signinSnippet(state, who)}\n`, { mode: 0o600 });
    writeFileSync(join(directory, 'state.json'), `${JSON.stringify(state, null, 2)}\n`);
    process.stdout.write(summary(state));
    process.stdout.write(`READY ${editor}\n`);
    // Held open until a signal: the children are this process's, and so is their cleanup.
    await new Promise<void>(() => undefined);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    await stopAll();
    process.exit(1);
  }
}

function subjectOf(n: number, who: PersonKey): string {
  return `b0b0b0b0-0000-4000-8000-${String(n).padStart(2, '0')}00000000${who}${who}`;
}

/** A Brand Kit for `tag`, so `$brand.*` tokens resolve and `brand.get` has something to answer. */
function brandFor(tag: string, accent: string): Record<string, unknown> {
  return {
    name: `${tag} brand`,
    colors: { primary: '#171717', secondary: '#4A4A4A', accent, background: '#F4F1EA', surface: '#FFFFFF', text: '#171717', text_inverse: '#F4F1EA', muted: '#8A857A' },
    typography: { display: 'Geist', body: 'Geist', mono: 'Geist Mono' },
    logo_asset_id: null,
    voice: `Plain and direct. Writes as ${tag}, never as anyone else.`,
    rules: { safe_margin: 64, minimum_font_size: 24 },
  };
}

function seededStub(email: string, subject: string, accountId: string): Seeded {
  return { email, subject, accountId, projectId: '', designs: [] };
}

/** The ids FakeProvider's scripts name (`test-driver/src/main.rs`): the first design's two headline layers. */
function fakeEnvironment(projectId: string, designs: readonly { id: string; name: string }[], first: unknown): Record<string, string> {
  const document = isRecord(first) && isRecord(first.document) ? first.document : {};
  const pages = Array.isArray(document.pages) ? (document.pages as unknown[]) : [];
  const layers = isRecord(pages[0]) && Array.isArray(pages[0].layers) ? (pages[0].layers as unknown[]) : [];
  const named = (name: string): string => {
    const layer = layers.find((candidate) => isRecord(candidate) && candidate.name === name);
    return isRecord(layer) && typeof layer.id === 'string' ? layer.id : fail(`the first design has no ${name} layer`);
  };
  return {
    FORMA_DRIVER_PROJECT: projectId,
    FORMA_DRIVER_DESIGN: designs[0]?.id ?? fail('no design'),
    FORMA_DRIVER_LONG_DESIGN: designs.at(-1)?.id ?? fail('no long design'),
    FORMA_DRIVER_LAYER: named('Headline'),
    FORMA_DRIVER_TAGLINE: named('Tagline'),
    FORMA_DRIVER_BASE_VERSION: String(isRecord(first) && typeof first.version === 'number' ? first.version : 1),
  };
}

async function pairingCode(sidecar: string, token: string): Promise<{ code: string; applicationId: string }> {
  const response = await fetch(`${sidecar}/pons/v1/pairing/codes`, {
    method: 'POST',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    body: '{}',
  });
  const raw = await response.text();
  if (!response.ok) fail(`pairing code: ${response.status} ${raw.slice(0, 300)}`);
  const parsed: unknown = JSON.parse(raw);
  if (!isRecord(parsed) || typeof parsed.code !== 'string' || typeof parsed.applicationId !== 'string') fail('pairing code: unexpected answer');
  return { code: parsed.code, applicationId: parsed.applicationId };
}

/** Fails closed: the editor Vite serves must be pointed at the fake provider, and at nothing else. */
async function assertFakeIdentity(editor: string, idp: string): Promise<void> {
  const served = await (await fetch(`${editor}/src/identity.ts`)).text();
  if (!served.includes(JSON.stringify(idp))) fail(`the editor was not built against the fake identity provider ${idp}; refusing to go on`);
  if (/supabase\.(co|com)/u.test(served)) fail('the editor module mentions a real Supabase host; refusing to go on');
}

async function assertProxies(editor: string): Promise<void> {
  const identity = await fetch(`${editor}/api/v1/identity`);
  if (!identity.ok) fail(`the /api proxy does not reach forma-server (${identity.status})`);
  // No bearer: the sidecar's own refusal, which a missing proxy route would replace with Vite's 404/HTML.
  const context = await fetch(`${editor}/agent/context`);
  if (context.status !== 401 && context.status !== 403) fail(`the Pontis proxy does not reach the sidecar (/agent/context answered ${context.status})`);
}

function summary(state: State): string {
  const lines: string[] = [];
  const out = (line = '') => lines.push(line);
  out(`Forma browser stack — instance ${state.instance}`);
  out();
  out(`  editor        ${state.urls.editor}      (also ${state.urls.editorAlias}, for a second signed-in person)`);
  out(`  forma-server  ${state.urls.forma}`);
  out(`  sidecar       ${state.urls.sidecar}`);
  out(`  identity      ${state.urls.idp}   (fake: the fixture JWKS; nothing reaches a real provider)`);
  out();
  for (const who of ['a', 'b'] as const) {
    const person = state.people[who];
    const provider = state.provider[who];
    out(`  person ${who.toUpperCase()}  ${person.email}   account ${person.accountId}`);
    out(`    project ${person.projectId}`);
    for (const design of person.designs) out(`    design  ${design.id}  ${design.name}`);
    out(`    runtime ${provider === null ? 'none (no device paired)' : provider === 'codex' ? 'real Codex 0.158.0, gpt-6-luna, effort low' : 'FakeProvider (scripted inputs only)'}`);
    out(`    sign in: node apps/pontis-sidecar/src/browser-stack.ts signin ${state.instance} ${who}`);
    out(`             or: orca eval --expression "$(cat ${join(state.directory, `signin-${who}.js`)})"   (on a tab at the editor URL)`);
  }
  out();
  out(`  logs   ${join(state.directory, 'logs')}/{forma-server,sidecar,vite,identity,driver-a,driver-b}.log`);
  out(`  state  ${join(state.directory, 'state.json')}`);
  out(`  stop   Ctrl-C here, or: node apps/pontis-sidecar/src/browser-stack.ts stop ${state.instance}`);
  out();
  return `${lines.join('\n')}\n`;
}

// --- Codex ------------------------------------------------------------------------------------------

/** The main checkout's `.codex-pinned`, shared by every worktree (the folder is gitignored). */
function pinnedDirectory(): string {
  if (process.env.FORMA_CODEX_PINNED) return resolve(process.env.FORMA_CODEX_PINNED);
  const common = spawnSync('git', ['rev-parse', '--path-format=absolute', '--git-common-dir'], { cwd: REPO, encoding: 'utf8' });
  return join(common.status === 0 ? dirname(common.stdout.trim()) : REPO, '.codex-pinned');
}

function codexPaths(): { pinned: string; bin: string; home: string } {
  const pinned = pinnedDirectory();
  return { pinned, bin: join(pinned, 'node_modules/.bin/codex'), home: join(pinned, 'home') };
}

function codexVersion(bin: string): string | undefined {
  const result = spawnSync(bin, ['--version'], { encoding: 'utf8' });
  return result.status === 0 ? result.stdout.trim() : undefined;
}

/** Everything real-Codex mode needs, checked without reading the credential. */
function checkCodex(): { bin: string; home: string; cold: boolean } {
  const { bin, home } = codexPaths();
  const missing = (what: string): never => fail(`${what} is missing: run \`node apps/pontis-sidecar/src/browser-stack.ts setup-codex\``);
  if (!existsSync(bin)) missing(`the pinned Codex at ${bin}`);
  if (codexVersion(bin) !== `codex-cli ${CODEX_VERSION}`) fail(`${bin} is not codex-cli ${CODEX_VERSION} (Pontis refuses other releases under capability_only); run setup-codex`);
  if (!existsSync(join(home, 'config.toml'))) missing(`${home}/config.toml`);
  // `existsSync` follows the symlink and reads nothing: it asks only whether the owner's credential is there.
  if (!existsSync(join(home, 'auth.json'))) missing(`${home}/auth.json (a symlink to ~/.codex/auth.json)`);
  if (!existsSync(BRIDGE)) missing(`the MCP bridge at ${BRIDGE}`);
  // Codex builds its state databases on first use, and the first session's `thread/start` on a cold
  // home outlasts the Runtime's timeout (observed: "timed out waiting for thread/start").
  return { bin, home, cold: !existsSync(join(home, 'state_5.sqlite')) };
}

/**
 * Opens a session on a brand-new `CODEX_HOME`, so the person's first message is not the one that
 * pays for its initialization. Costs nothing — no prompt is sent — and the first attempt is
 * expected to time out; the second finds the home warm.
 */
async function warmCodex(sidecar: string, token: string): Promise<void> {
  const headers = { authorization: `Bearer ${token}`, 'content-type': 'application/json' };
  const read = async (path: string, init?: RequestInit): Promise<unknown> => {
    const response = await fetch(`${sidecar}${path}`, { headers, ...init });
    const parsed: unknown = await response.json();
    return parsed;
  };
  const context = await read('/agent/context');
  const contextId = isRecord(context) && typeof context.applicationContextId === 'string' ? context.applicationContextId : fail('no application context');
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    const started = await read('/agent/intent', { method: 'POST', body: JSON.stringify({ type: 'session.start', payload: { providerId: 'codex' } }) });
    const sessionId = isRecord(started) && typeof started.sessionId === 'string' ? started.sessionId : fail('the warm-up session did not start');
    let outcome: 'started' | 'failed' | undefined;
    for (let poll = 0; poll < 240 && outcome === undefined; poll += 1) {
      const page = await read(`/pons/v1/events?scope=application_context&applicationContextId=${contextId}&after=0`);
      const events = isRecord(page) && Array.isArray(page.events) ? (page.events as unknown[]) : [];
      for (const event of events) {
        if (!isRecord(event) || event.sessionId !== sessionId) continue;
        if (event.type === 'session.started') outcome = 'started';
        if (event.type === 'session.failed') outcome = 'failed';
      }
      if (outcome === undefined) await new Promise((done) => setTimeout(done, 500));
    }
    if (outcome === 'started') {
      await read('/agent/intent', { method: 'POST', body: JSON.stringify({ type: 'session.cancel', payload: { sessionId } }) });
      return;
    }
  }
  process.stdout.write('warning: Codex never started a warm-up session; the first message may fail (see logs/driver-*.log)\n');
}

function setupCodex(): void {
  const { pinned, bin, home } = codexPaths();
  const step = (what: string) => process.stdout.write(`${what}\n`);
  if (codexVersion(bin) !== `codex-cli ${CODEX_VERSION}`) {
    step(`installing @openai/codex@${CODEX_VERSION} into ${pinned} (the global codex is not touched)`);
    mkdirSync(pinned, { recursive: true });
    const installed = spawnSync('npm', ['install', '--prefix', pinned, `@openai/codex@${CODEX_VERSION}`], { stdio: 'inherit' });
    if (installed.status !== 0) fail('npm install failed');
  }
  const version = codexVersion(bin);
  if (version !== `codex-cli ${CODEX_VERSION}`) fail(`expected codex-cli ${CODEX_VERSION}, got ${String(version)}`);
  step(version);

  mkdirSync(home, { recursive: true });
  const config = join(home, 'config.toml');
  if (!existsSync(config) || readFileSync(config, 'utf8') !== CODEX_CONFIG) {
    writeFileSync(config, CODEX_CONFIG);
    step(`wrote ${config}`);
  }
  // A symlink, never a copy: the credential is the owner's, and nothing here opens it.
  const auth = join(home, 'auth.json');
  const target = join(homedir(), '.codex/auth.json');
  const link = (() => {
    try {
      return lstatSync(auth).isSymbolicLink() ? readlinkSync(auth) : 'not a symlink';
    } catch {
      return undefined;
    }
  })();
  if (link === undefined) {
    if (!existsSync(target)) fail(`${target} does not exist: sign in with the global codex first`);
    symlinkSync(target, auth);
    step(`linked ${auth} -> ${target}`);
  } else if (link !== target) {
    fail(`${auth} already exists and is not a symlink to ${target}; remove it and run again`);
  }

  if (!existsSync(BRIDGE)) {
    // The checkout the sidecar's own dependencies resolve into (README: "The Pontis dependency"),
    // found through the package rather than by naming the seam a second time.
    const pontis = process.env.FORMA_PONTIS_CHECKOUT ?? resolve(realpathSync(join(PACKAGE, 'node_modules/@pontis/core')), '../..');
    if (!existsSync(join(pontis, 'Cargo.toml'))) fail(`no Pontis checkout at ${pontis}: see apps/pontis-sidecar/README.md, or set FORMA_PONTIS_CHECKOUT`);
    step(`building pontis-mcp-bridge from the Pontis checkout into ${join(PACKAGE, 'test-driver/target')}`);
    const built = spawnSync('cargo', ['build', '-p', 'pontis-runtime', '--bin', 'pontis-mcp-bridge', '--target-dir', join(PACKAGE, 'test-driver/target')], { cwd: pontis, stdio: 'inherit' });
    if (built.status !== 0) fail('building the bridge failed');
  }
  step(`ready: ${bin}, CODEX_HOME ${home}, bridge ${BRIDGE}`);
}

// --- the other commands -------------------------------------------------------------------------------

function readState(n: number): State {
  const path = join(instanceDirectory(n), 'state.json');
  if (!existsSync(path)) fail(`instance ${n} is not running (no ${path})`);
  const parsed: unknown = JSON.parse(readFileSync(path, 'utf8'));
  return parsed as State;
}

async function stopInstance(n: number): Promise<void> {
  const state = readState(n);
  try {
    process.kill(state.pid, 'SIGTERM');
  } catch {
    // The harness is gone but its children may not be: stop their groups directly.
    for (const { pid } of [...state.processes].reverse()) {
      try {
        process.kill(-pid, 'SIGTERM');
      } catch {
        // already gone
      }
    }
    rmSync(join(state.directory, 'state.json'), { force: true });
    process.stdout.write(`instance ${n}: harness was gone; stopped what was left\n`);
    return;
  }
  await until(`instance ${n} to stop`, async () => await portIsFree(state.ports.forma ?? 0));
  process.stdout.write(`instance ${n} stopped\n`);
}

async function main(): Promise<void> {
  const [command, rawInstance, ...rest] = process.argv.slice(2);
  switch (command) {
    case 'start':
      return await start(parseInstance(rawInstance), parseOptions(rest));
    case 'stop':
      return await stopInstance(parseInstance(rawInstance));
    case 'signin': {
      const who = rest[0];
      if (who !== 'a' && who !== 'b') usage();
      process.stdout.write(`${signinSnippet(readState(parseInstance(rawInstance)), who)}\n`);
      return;
    }
    case 'setup-codex':
      return setupCodex();
    default:
      return usage();
  }
}

main().catch((error: unknown) => {
  process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
  process.exit(1);
});
