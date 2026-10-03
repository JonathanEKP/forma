/**
 * The application-owned integration sidecar of ADR-023 (plan §2, §7.1).
 *
 * One process, one HTTP server, holding the four Pontis components ADR-016 and
 * ADR-023 keep together — the Capability Gateway, the delegation issuer, the
 * Control Plane and the Realtime Gateway — so the opaque credentials of ADR-015
 * are issued and validated in the same place. Every one of them is a public
 * Pontis package mounted as it ships; what is Forma's is the capabilities, the
 * Boundary F client they call, how a person is resolved, and the intent route.
 *
 * It reaches Forma only through Boundary F, with its own service credential.
 * forma-server stays the domain authority and has no dependency on this
 * process: with the sidecar down, the editor, the REST API and forma-mcp are
 * unaffected (§7.3).
 */
import { createServer, type IncomingMessage, type Server, type ServerResponse } from 'node:http';

import { ControlPlane, DeviceGateway, bearerToken, createControlPlaneRoutes } from '@pontis/control-plane';
import { createRemoteGatewayHandler, createUiRequestHandler, defineAgentApp, type AgentApp } from '@pontis/server';

import { FormaBoundary } from './boundary.ts';
import { formaCapabilities, isUuid, type FormaUser } from './capabilities.ts';
import { GUIDANCE } from './guidance.ts';
import { createContextRoute, createIntentRoute } from './intents.ts';
import type { Log } from './log.ts';
import { personResolver } from './person.ts';
import { uiCapabilities } from './ui.ts';

/** The Pontis application id this deployment serves (one deployment, one application). */
export const APPLICATION_ID = 'forma';

export interface SidecarOptions {
  /** Where forma-server listens, e.g. `http://127.0.0.1:4765`. */
  readonly formaUrl: string;
  /** The `forma_svc_` credential minted by `forma-server service-credential`. */
  readonly serviceCredential: string;
  readonly host: string;
  /** 0 picks a free port. */
  readonly port: number;
  /**
   * The public origin Devices reach this process at, which is the audience of
   * every Device proof. Defaults to the address bound, which is right only for
   * a local run; behind Caddy it is the public origin.
   */
  readonly origin?: string;
  /**
   * The origins a browser may reach the UI routes from (plan §10(b) D9).
   *
   * Same-origin by construction: the editor and these three routes arrive
   * through one proxy — Caddy in the deployment, Vite in development — so the
   * single right value is the origin the editor is served from. The transport
   * derives the acceptable `Host` values from this same list, which is its
   * DNS-rebinding defence, and refuses everything when the list is empty.
   *
   * Omitted means no UI channel: the four `ui.*` capabilities stay registered
   * and answer `ui_unavailable`, which is the honest answer when no window can
   * connect.
   */
  readonly editorOrigins?: readonly string[];
  readonly log: Log;
}

export interface Sidecar {
  readonly origin: string;
  readonly agentApp: AgentApp;
  readonly controlPlane: ControlPlane;
  close(): Promise<void>;
}

export async function startSidecar(options: SidecarOptions): Promise<Sidecar> {
  const server = createServer();
  const port = await listen(server, options.host, options.port);
  const origin = options.origin ?? `http://${options.host}:${port}`;

  const boundary = new FormaBoundary(options.formaUrl, options.serviceCredential, options.log);
  const resolvePerson = personResolver(boundary);

  const agentApp = defineAgentApp({
    id: APPLICATION_ID,
    guidance: GUIDANCE,
    capabilities: [...formaCapabilities(boundary), ...uiCapabilities(boundary)],
    // The represented user arrives inside a delegation this process issued
    // for a person it resolved through Forma, so the id is a Forma subject.
    // Whether that subject may still act in the session's account is not
    // decided here: Boundary F revalidates the membership on every call.
    resolveUser: (userId): FormaUser | undefined => (isUuid(userId) ? { id: userId } : undefined),
  });

  const controlPlane = new ControlPlane({ applicationId: APPLICATION_ID, audience: origin });
  const devices = new DeviceGateway({ controlPlane });
  devices.attach(server);

  const controlRoutes = createControlPlaneRoutes({
    controlPlane,
    resolveAppUser: async (request) => {
      const person = await resolvePerson(request);
      return person ? { userId: person.userId } : undefined;
    },
    resolveApplicationContext: async (request, user) => {
      const person = await resolvePerson(request);
      return person && person.userId === user.userId ? person.applicationContextId : undefined;
    },
  });
  const intentRoute = createIntentRoute({ controlPlane, devices, resolvePerson, log: options.log });
  const contextRoute = createContextRoute({ resolvePerson });

  // The frontend hop, and a genuinely different one from the gateway's: the
  // caller is a browser, so `Origin` is always present and is checked, where the
  // Runtime's hop refuses any request that carries one. The person behind it is
  // resolved exactly as the Control Plane routes resolve one — a Forma session
  // token, answered by `/service/resolve` — so a tab authenticated for another
  // account resolves a different `applicationContextId` and is not an eligible
  // target for this one's actions (ADR-024 Decision 2).
  const uiRoute = createUiRequestHandler({
    clients: agentApp.uiClients,
    router: agentApp.uiRouter,
    authenticatePrincipal: async (request) => {
      const person = await resolvePerson(request);
      // The app session is the person in their account: Forma issues no session
      // id of its own to a browser, and the provider's JWT is not one to borrow
      // — it is refreshed under the page, so a reload would look like a new
      // client while the old one stayed registered.
      return person
        ? { userId: person.userId, appSessionId: `${person.userId}:${person.applicationContextId}` }
        : undefined;
    },
    resolveApplicationContext: async (request, user) => {
      const person = await resolvePerson(request);
      return person && person.userId === user.userId ? person.applicationContextId : undefined;
    },
    ...(options.editorOrigins?.length ? { allowedOrigins: [...options.editorOrigins] } : {}),
  });
  const gatewayRoutes = createRemoteGatewayHandler({
    gateway: agentApp.gateway,
    applicationId: APPLICATION_ID,
    // The co-located delegation authority of ADR-015: the Runtime's bearer is
    // an AppDelegation this Control Plane issued, scoped to one user, session,
    // context and capability set.
    authenticate: async ({ request, representedUser, capabilityId }) => {
      const check = await controlPlane.validateDelegation({
        delegation: bearerToken(request),
        userId: representedUser.userId,
        sessionId: representedUser.sessionId,
        applicationContextId: representedUser.applicationContextId,
        ...(capabilityId ? { capabilityId } : {}),
      });
      return check.ok ? { ok: true } : { ok: false, error: { code: check.code, category: 'auth', message: check.message } };
    },
  });

  const dispatch = async (request: IncomingMessage, response: ServerResponse): Promise<void> => {
    if (await controlRoutes(request, response)) return;
    if (await intentRoute(request, response)) return;
    if (await contextRoute(request, response)) return;
    if (await uiRoute(request, response)) return;
    if (await gatewayRoutes(request, response)) return;
    respond(response, 404, { error: 'not_found' });
  };
  server.on('request', (request: IncomingMessage, response: ServerResponse) => {
    dispatch(request, response).catch(() => {
      options.log('request_failed', { path: (request.url ?? '').split('?')[0] });
      respond(response, 500, { error: 'internal_error' });
    });
  });

  options.log('sidecar_started', {
    origin,
    forma: options.formaUrl,
    capabilities: agentApp.registry.ids().join(','),
    editorOrigins: options.editorOrigins?.join(',') ?? '',
  });

  return {
    origin,
    agentApp,
    controlPlane,
    close: async () => {
      const closed = await Promise.allSettled([
        devices.close(),
        new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve()))),
      ]);
      server.closeAllConnections();
      if (closed.some((result) => result.status === 'rejected')) throw new Error('the sidecar did not close cleanly');
    },
  };
}

async function listen(server: Server, host: string, port: number): Promise<number> {
  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, host, () => {
      server.off('error', reject);
      resolve();
    });
  });
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('the sidecar did not bind a TCP port');
  return address.port;
}

function respond(response: ServerResponse, status: number, body: unknown): void {
  if (response.headersSent || response.destroyed) {
    response.destroy();
    return;
  }
  const bytes = Buffer.from(JSON.stringify(body));
  response.writeHead(status, {
    'content-type': 'application/json',
    'content-length': String(bytes.length),
    'cache-control': 'no-store',
  });
  response.end(bytes);
}
