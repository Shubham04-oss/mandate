// Reads the verified assertion that route auth put on the session's caller.
import type { Assertion } from "./assertion.ts";

interface AuthLike {
  readonly attributes: Readonly<Record<string, string | readonly string[]>>;
}

export const AUTHENTICATOR = "mandate-core";

function attr(auth: AuthLike, name: string): string | null {
  const v = auth.attributes[name];
  return typeof v === "string" ? v : null;
}

export function assertionOf(auth: AuthLike | null | undefined): Assertion | null {
  if (!auth) return null;
  const tenant = attr(auth, "tenant");
  const member = attr(auth, "member");
  const agent = attr(auth, "agent");
  const expires = Number(attr(auth, "expires_at"));
  if (!tenant || !member || !agent || !Number.isInteger(expires)) return null;
  return { tenant, member, agent, expires_at: expires };
}

export function tokenOf(auth: AuthLike | null | undefined): string | null {
  return auth ? attr(auth, "assertion") : null;
}
