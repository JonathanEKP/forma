/**
 * What survives the sidecar's own restart (fix batch 1, Task C; findings F-10-1, F-chat-6, F-40-10), and
 * what a stop has to do with an editor window still open (F-40-2).
 *
 * Nothing mocks the disk: each case writes under a real temporary directory, closes the sidecar and
 * starts a new one on the same directory, then reads back what the person would have seen. The
 * stores are Pontis's own journals, so what is Forma's here is only that they are composed in, where
 * they are put and who may read them.
 */
import { strict as assert } from 'node:assert';
import { generateKeyPairSync } from 'node:crypto';
import { mkdtemp, readdir, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, describe, it } from 'node:test';

import { SUBJECT_TOKEN_HEADER } from './boundary.ts';
import { envelope, fakeForma } from './fake-forma.ts';
import { silentLog, type LogFields } from './log.ts';
import { APPLICATION_ID, startSidecar, type Sidecar } from './sidecar.ts';
import { UI_ACTIONS } from './ui.ts';
import { connect as connectWindow } from './ui-window.ts';

const USER = '11111111-1111-4111-8111-111111111111';
const CONTEXT = '22222222-2222-4222-8222-222222222222';
const ORIGIN = 'http://editor.test';

const directories: string[] = [];
after(async () => {
  for (const path of directories) await rm(path, { recursive: true, force: true });
});

async function scratch(): Promise<string> {
  const path = await mkdtemp(join(tmpdir(), 'forma-pontis-state-'));
  directories.push(path);
  return path;
}

async function forma() {
  return await fakeForma((request) => {
    if (request.path !== '/api/v1/service/resolve') return { status: 404, body: envelope('NO_SUCH_OPERATION', 'not_found') };
    return request.headers[SUBJECT_TOKEN_HEADER] === 'session-user'
      ? { status: 200, body: { subject: USER, account_id: CONTEXT, role: 'owner' } }
      : { status: 401, body: envelope('UNAUTHORIZED', undefined) };
  });
}

async function start(formaUrl: string, dataDir: string | undefined, log = silentLog): Promise<Sidecar> {
  return await startSidecar({
    formaUrl,
    serviceCredential: 'forma_svc_test',
    host: '127.0.0.1',
    port: 0,
    editorOrigins: [ORIGIN],
    ...(dataDir ? { dataDir } : {}),
    log,
  });
}

function publicKey(): string {
  const jwk = generateKeyPairSync('ed25519').publicKey.export({ format: 'jwk' });
  assert.ok(typeof jwk.x === 'string');
  return jwk.x;
}

describe('a restart on the same data directory', () => {
  it('keeps the paired device, the unclaimed pairing code and the conversation', async () => {
    const server = await forma();
    const dataDir = await scratch();
    const before = await start(server.url, dataDir);
    let deviceId: string;
    let unclaimed: string;
    try {
      const { code } = await before.controlPlane.createPairingCode(USER);
      const device = await before.controlPlane.claimPairing({
        code,
        applicationId: APPLICATION_ID,
        publicKey: publicKey(),
        metadata: { name: 'Workstation', platform: 'linux', runtimeVersion: '0.1.0' },
      });
      deviceId = device.id;
      unclaimed = (await before.controlPlane.createPairingCode(USER)).code;
      await before.controlPlane.appendEvent({
        userId: USER,
        applicationContextId: CONTEXT,
        type: 'agent.output.completed',
        payload: { turnId: 'turn_1', text: 'Done.' },
      });
    } finally {
      await before.close();
    }

    const after = await start(server.url, dataDir);
    try {
      assert.deepEqual((await after.controlPlane.listDevices(USER)).map((device) => device.id), [deviceId]);

      const history = await after.controlPlane.events.readAfter(after.controlPlane.streamKeyFor(USER, CONTEXT), 0, 10);
      assert.deepEqual(history.map((event) => event.payload), [{ turnId: 'turn_1', text: 'Done.' }]);

      // A code still in date is still a code: the person who minted it has not been told otherwise.
      const claimed = await after.controlPlane.claimPairing({
        code: unclaimed,
        applicationId: APPLICATION_ID,
        publicKey: publicKey(),
        metadata: { name: 'Laptop', platform: 'linux', runtimeVersion: '0.1.0' },
      });
      assert.equal(claimed.userId, USER);
    } finally {
      await after.close();
      await server.close();
    }
  });

  it('keeps a revocation, which is the record that must never be forgotten', async () => {
    const server = await forma();
    const dataDir = await scratch();
    const first = await start(server.url, dataDir);
    let deviceId: string;
    try {
      const { code } = await first.controlPlane.createPairingCode(USER);
      const device = await first.controlPlane.claimPairing({
        code,
        applicationId: APPLICATION_ID,
        publicKey: publicKey(),
        metadata: { name: 'Lost laptop', platform: 'linux', runtimeVersion: '0.1.0' },
      });
      deviceId = device.id;
      await first.controlPlane.revokeDevice(USER, deviceId);
    } finally {
      await first.close();
    }

    const second = await start(server.url, dataDir);
    try {
      const device = await second.controlPlane.getDevice(deviceId);
      assert.ok(device?.revokedAt, 'the device is still revoked');
    } finally {
      await second.close();
      await server.close();
    }
  });

  it('writes owner-only files in an owner-only directory', async () => {
    const server = await forma();
    const dataDir = join(await scratch(), 'nested', 'pontis');
    const sidecar = await start(server.url, dataDir);
    try {
      await sidecar.controlPlane.createPairingCode(USER);
      assert.equal((await stat(dataDir)).mode & 0o777, 0o700);
      const files = await readdir(dataDir);
      assert.ok(files.length >= 2, `a journal per store, got ${files.join(', ')}`);
      for (const file of files) assert.equal((await stat(join(dataDir, file))).mode & 0o777, 0o600, file);
    } finally {
      await sidecar.close();
      await server.close();
    }
  });
});

describe('with no data directory', () => {
  it('says out loud that state will not survive a restart, and forgets it', async () => {
    const server = await forma();
    const lines: { event: string; fields: LogFields | undefined }[] = [];
    const first = await start(server.url, undefined, (event, fields) => lines.push({ event, fields }));
    try {
      await first.controlPlane.createPairingCode(USER);
    } finally {
      await first.close();
    }
    const warning = lines.find((line) => line.event === 'pontis_state_in_memory');
    assert.ok(warning, 'a warn line names the consequence');
    assert.equal(warning.fields?.level, 'warn');

    const second = await start(server.url, undefined);
    try {
      assert.deepEqual(await second.controlPlane.listDevices(USER), []);
    } finally {
      await second.close();
      await server.close();
    }
  });
});

describe('a stop with an editor window open', () => {
  it('ends the window\'s stream and completes, rather than waiting for the person to close the tab', async () => {
    const server = await forma();
    const sidecar = await start(server.url, undefined);
    const window = await connectWindow(sidecar.origin, 'session-user', ORIGIN, UI_ACTIONS);
    try {
      const started = Date.now();
      await Promise.race([
        sidecar.close(),
        new Promise<never>((_, reject) => setTimeout(() => reject(new Error('close() did not complete while a window was open')), 3_000)),
      ]);
      assert.ok(Date.now() - started < 3_000);
    } finally {
      window.close();
      await server.close();
    }
  });
});
