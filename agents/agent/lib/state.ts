// The turn token the core's registry issued, kept per session, and renewed before it runs out.
import type { KeyObject } from "node:crypto";
import { defineState } from "eve/context";
import { type Assertion, coreKey, readAssertion } from "./assertion.ts";
import { identityOf } from "./caller.ts";
import { corePrivateUrl } from "./core.ts";

/** A `turn` token for one member's turn in this session's conversation. */
export interface TurnGrant {
  token: string;
  expires_at: number;
  member: string;
}

export const turnGrant = defineState<TurnGrant | null>("mandate.turn", () => null);

/** A token closer than this to expiry is renewed first, so a call never starts with a dying one. */
export const MIN_LEFT_SECS = 30;

/**
 * The core's `{ assertion, expires_at }` as a grant, only if it is a signed `turn` token for
 * `member`. Anything else throws, which fails the turn.
 */
export function grantFrom(body: unknown, key: KeyObject, member: string | undefined): TurnGrant {
  const token = (body as { assertion?: unknown } | null)?.assertion;
  const a = typeof token === "string" ? readAssertion(token, key) : null;
  if (a?.purpose !== "turn" || a.member === undefined || a.member !== member) {
    throw new Error("the core returned no valid turn token for this caller");
  }
  return { token: token as string, expires_at: a.expires_at, member: a.member };
}

/** POST a JSON body to a core private route with a bearer token; the JSON answer, or a throw. */
export async function postCore(
  url: string,
  bearer: string,
  body: unknown,
  fetchFn: typeof fetch = fetch,
): Promise<unknown> {
  const res = await fetchFn(url, {
    method: "POST",
    headers: { authorization: `Bearer ${bearer}`, "content-type": "application/json" },
    body: JSON.stringify(body),
    // The bearer never follows a redirect anywhere.
    redirect: "error",
  });
  // Status only: the core's message is not needed here and could carry request detail.
  if (!res.ok) throw new Error(`core answered ${res.status}`);
  return res.json();
}

/** The held grant when it is the caller's and fresh; renewed through the core when it is near expiry. */
export async function freshGrant(
  current: Assertion | null,
  held: TurnGrant | null,
  nowSecs: number,
  renew: (held: TurnGrant) => Promise<TurnGrant>,
): Promise<TurnGrant> {
  if (current?.member === undefined) throw new Error("this turn has no member caller");
  // Another member's turn token never stands in for this caller.
  if (!held || held.member !== current.member) throw new Error("no turn token for this caller");
  if (held.expires_at - nowSecs >= MIN_LEFT_SECS) return held;
  return renew(held);
}

/** `POST /private/v1/assertions/renew` (docs/channels.md §6.3). */
export async function renewGrant(
  base: string,
  sessionId: string,
  held: TurnGrant,
  key: KeyObject,
  fetchFn: typeof fetch = fetch,
): Promise<TurnGrant> {
  const body = await postCore(
    `${base}/private/v1/assertions/renew`,
    held.token,
    { session_id: sessionId },
    fetchFn,
  );
  return grantFrom(body, key, held.member);
}

interface SessionLike {
  readonly id: string;
  readonly auth: { readonly current: Parameters<typeof identityOf>[0] };
}

/** The current caller's turn token from session state, renewed and saved when near expiry. */
export async function currentGrant(session: SessionLike): Promise<TurnGrant> {
  const key = coreKey();
  const held = turnGrant.get();
  const grant = await freshGrant(
    identityOf(session.auth.current, key),
    held,
    Math.floor(Date.now() / 1000),
    (h) => renewGrant(corePrivateUrl(), session.id, h, key),
  );
  if (grant !== held) turnGrant.update(() => grant);
  return grant;
}
