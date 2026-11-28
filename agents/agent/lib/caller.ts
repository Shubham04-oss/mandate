// Who a turn runs as: read only from a token whose signature verifies, never from loose attributes.
import type { KeyObject } from "node:crypto";
import { type Assertion, readAssertion } from "./assertion.ts";

interface AuthLike {
  readonly attributes: Readonly<Record<string, string | readonly string[]>>;
}

export const AUTHENTICATOR = "mandate-core";

/** Where a turn came in, as the `channel` attribute names it (docs/channels.md §5). */
export type ChannelAttr = "web" | "slack" | "whatsapp" | "sms";

// Every attribute the core's assertion sets; a channel's own same-named attribute never survives.
const MANDATE_ATTRIBUTES = [
  "tenant",
  "principal",
  "member",
  "agent",
  "space",
  "conversation",
  "thread",
  "purpose",
  "expires_at",
  "channel",
  "assertion",
] as const;

export function tokenOf(auth: AuthLike | null | undefined): string | null {
  const v = auth?.attributes.assertion;
  return typeof v === "string" ? v : null;
}

/**
 * The caller's signed assertion (signature checked, expiry ignored: the core checks every use),
 * or null. Planted `tenant` or `member` attributes without a valid token count for nothing.
 */
export function identityOf(auth: AuthLike | null | undefined, key: KeyObject): Assertion | null {
  const token = tokenOf(auth);
  return token ? readAssertion(token, key) : null;
}

/** A channel's attributes with the core's set over them; any same-named one is replaced. */
export function mandateAttributes(
  base: Readonly<Record<string, string | readonly string[]>>,
  a: Assertion,
  token: string,
  channel: ChannelAttr,
): Record<string, string | readonly string[]> {
  const out: Record<string, string | readonly string[]> = { ...base };
  for (const name of MANDATE_ATTRIBUTES) delete out[name];
  for (const name of ["member", "space", "conversation", "thread"] as const) {
    const v = a[name];
    if (v !== undefined) out[name] = v;
  }
  out.tenant = a.tenant;
  out.principal = a.principal;
  out.agent = a.agent;
  out.purpose = a.purpose;
  out.expires_at = String(a.expires_at);
  out.channel = channel;
  out.assertion = token;
  return out;
}

/** The mandate connection's instance: the verified tenant and agent ids (stable, not secret). */
export function instanceKeyOf(a: Assertion): string {
  return `${a.tenant}/${a.agent}`;
}
