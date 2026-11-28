// eve's provider channels behind the core (docs/channels.md §6, §12): inbound only through the
// core's forward assertion, outbound only through the core's proxy, with the proxy key.
import type { KeyObject } from "node:crypto";
import type { Channel, RouteHandlerArgs } from "eve/channels";
import { extractBearerToken } from "eve/channels/auth";
import { type Assertion, bodyHash, coreKey, verifyAssertion } from "./assertion.ts";
import { type ChannelAttr, mandateAttributes } from "./caller.ts";
import { corePrivateUrl } from "./core.ts";

export const PROXY_KEY_HEADER = "x-mandate-proxy-key";

/**
 * The core's forward assertion on this request, or null: a valid, unexpired `forward` token whose
 * `body` hash matches the bytes eve received. No other purpose is accepted here.
 */
export function verifyForward(
  request: Request,
  body: string | Uint8Array,
  key: KeyObject = coreKey(),
  nowSecs: number = Math.floor(Date.now() / 1000),
): Assertion | null {
  const token = extractBearerToken(request.headers.get("authorization"));
  const a = token ? verifyAssertion(token, key, nowSecs) : null;
  if (a?.purpose !== "forward" || a.body === undefined) return null;
  return a.body === bodyHash(body) ? a : null;
}

// biome-ignore lint/suspicious/noExplicitAny: the same bound eve uses for any channel (isChannel).
type AnyChannel = Channel<any, any, any>;

/**
 * The channel with every HTTP route behind the core's forward assertion (401 before eve's
 * handler runs), and each inbound message's auth keeping the provider identity while the core's
 * attributes (tenant, member, agent, space, conversation...) replace any same-named ones.
 */
export function wrapChannel<C extends AnyChannel>(
  channel: C,
  kind: ChannelAttr,
  key: () => KeyObject = coreKey,
): C {
  const routes = channel.routes.map((route) =>
    route.transport === "websocket"
      ? route
      : {
          ...route,
          handler: async (request: Request, args: RouteHandlerArgs<unknown>) => {
            const body = new Uint8Array(await request.clone().arrayBuffer());
            const token = extractBearerToken(request.headers.get("authorization"));
            const a = token ? verifyForward(request, body, key()) : null;
            if (!token || !a) return new Response("unauthorized", { status: 401 });
            return route.handler(request, withMandateAuth(args, a, token, kind));
          },
        },
  );
  return { ...channel, routes };
}

function withMandateAuth(
  args: RouteHandlerArgs<unknown>,
  a: Assertion,
  token: string,
  kind: ChannelAttr,
): RouteHandlerArgs<unknown> {
  return {
    ...args,
    from(address) {
      const source = args.from(address);
      return {
        ...source,
        send(message, options) {
          // Only a sender the provider identified gets a turn; the core's ids go on top.
          if (!options.auth) throw new Error("an inbound message needs its sender's auth");
          const auth = {
            ...options.auth,
            attributes: mandateAttributes(options.auth.attributes, a, token, kind),
          };
          return source.send(message, { ...options, auth });
        },
      };
    },
  };
}

// RFC 6761: never resolves. `eve build` evaluates channel files without the runtime environment,
// and the startup check (lib/env.ts) refuses to run without MANDATE_CORE_PRIVATE_URL.
const BUILD_PLACEHOLDER = "http://core.invalid";

/** The core's private base for channel API bases; a placeholder only while eve builds. */
export function channelProxyBase(env: NodeJS.ProcessEnv = process.env): string {
  return env.MANDATE_CORE_PRIVATE_URL ? corePrivateUrl(env) : BUILD_PLACEHOLDER;
}

/**
 * A channel's `api.fetch`: adds the per-environment proxy key to requests for the core's private
 * origin, and to nothing else. Such requests never follow a redirect, so the key stays there.
 */
export function withProxyKey(
  base: typeof fetch = fetch,
  env: NodeJS.ProcessEnv = process.env,
): typeof fetch {
  return (input, init) => {
    const target = URL.parse(input instanceof Request ? input.url : String(input));
    if (target?.origin !== new URL(corePrivateUrl(env)).origin) return base(input, init);
    const proxyKey = env.MANDATE_CHANNEL_PROXY_KEY;
    if (!proxyKey) throw new Error("MANDATE_CHANNEL_PROXY_KEY is not set");
    const headers = new Headers(init?.headers ?? (input instanceof Request ? input.headers : {}));
    headers.set(PROXY_KEY_HEADER, proxyKey);
    return base(input, { ...init, headers, redirect: "error" });
  };
}
