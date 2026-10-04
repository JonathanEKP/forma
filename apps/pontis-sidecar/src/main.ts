/**
 * `forma-pontis`: the sidecar as a process.
 *
 * Configured from the environment, like forma-server and forma-mcp:
 *
 * ```text
 * FORMA_SERVER_URL=http://127.0.0.1:4765
 * FORMA_SERVICE_CREDENTIAL=forma_svc_…    from `forma-server service-credential <name>`
 * FORMA_PONTIS_BIND=127.0.0.1:4767       4766 is forma-mcp's (plan §2)
 * FORMA_PONTIS_ORIGIN=                   the public origin, when behind a proxy
 * FORMA_PONTIS_EDITOR_ORIGIN=            where the editor is served from, comma
 *                                        separated; the UI channel's allowlist
 * FORMA_PONTIS_DATA_DIR=                 where devices, pairing codes and the event
 *                                        history are kept; unset means memory, and a
 *                                        restart forgets all of it (logged at warn)
 * ```
 *
 * Prints `READY <origin>` on stdout once it is listening, and logs JSON lines on
 * stderr. Stops on SIGINT or SIGTERM.
 */
import { stderrLog } from './log.ts';
import { startSidecar } from './sidecar.ts';

const DEFAULT_BIND = '127.0.0.1:4767';

function required(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) {
    process.stderr.write(`${name} is required\n`);
    process.exit(2);
  }
  return value;
}

const bind = process.env.FORMA_PONTIS_BIND?.trim() || DEFAULT_BIND;
const separator = bind.lastIndexOf(':');
const host = bind.slice(0, separator);
const port = Number(bind.slice(separator + 1));
if (separator <= 0 || !Number.isInteger(port) || port < 0 || port > 65_535) {
  process.stderr.write(`FORMA_PONTIS_BIND ${JSON.stringify(bind)} is not host:port\n`);
  process.exit(2);
}
// The same rule as forma-server's: reaching the network is something a
// deployment opts into through a proxy, never a default.
if (host !== '127.0.0.1' && host !== '::1' && host !== 'localhost') {
  process.stderr.write(`FORMA_PONTIS_BIND must be a loopback address; put a proxy in front for anything else\n`);
  process.exit(2);
}

const origin = process.env.FORMA_PONTIS_ORIGIN?.trim();
// The editor and the UI routes share an origin through the proxy in front of
// both (plan §10(b) D9), so the usual value is the one public name. Unset means
// no UI channel rather than an open one: the transport refuses every origin when
// its allowlist is empty, which is the fail-closed reading of a missing setting.
const editorOrigins = (process.env.FORMA_PONTIS_EDITOR_ORIGIN?.trim() ?? '')
  .split(',')
  .map((value) => value.trim())
  .filter((value) => value.length > 0);
const dataDir = process.env.FORMA_PONTIS_DATA_DIR?.trim();
const sidecar = await startSidecar({
  formaUrl: required('FORMA_SERVER_URL'),
  serviceCredential: required('FORMA_SERVICE_CREDENTIAL'),
  host,
  port,
  ...(origin ? { origin } : {}),
  ...(editorOrigins.length ? { editorOrigins } : {}),
  ...(dataDir ? { dataDir } : {}),
  log: stderrLog,
});
process.stdout.write(`READY ${sidecar.origin}\n`);

await new Promise<void>((resolve) => {
  process.once('SIGINT', resolve);
  process.once('SIGTERM', resolve);
});
stderrLog('sidecar_stopping');
await sidecar.close();
