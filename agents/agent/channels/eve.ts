// Route auth: only the core reaches eve, and only with its signed assertion: a `turn` token for
// web turns, or an `answer` token that may only answer an approval.

import type { KeyObject } from "node:crypto";
import { extractBearerToken, UnauthenticatedError } from "eve/channels/auth";
import { defaultEveAuth, type EveMessageContext, eveChannel } from "eve/channels/eve";
import { coreKey, verifyAssertion } from "#lib/assertion.ts";
import { AUTHENTICATOR, mandateAttributes } from "#lib/caller.ts";

/** The session auth for a request carrying a valid `turn` or `answer` token, else null (401). */
export function mandateRouteAuth(request: Request, key: KeyObject, nowSecs: number) {
  const token = extractBearerToken(request.headers.get("authorization"));
  const a = token ? verifyAssertion(token, key, nowSecs) : null;
  if (!token || !a || a.member === undefined) return null;
  if (a.purpose !== "turn" && a.purpose !== "answer") return null;
  return {
    authenticator: AUTHENTICATOR,
    issuer: "mandate",
    principalType: "user",
    principalId: `${a.tenant}:${a.member}`,
    attributes: mandateAttributes({}, a, token, "web"),
  };
}

/** A message needs a `turn` token: an `answer` token only answers (answers skip `onMessage`). */
export function mandateOnMessage(ctx: EveMessageContext) {
  if (ctx.eve.caller?.attributes.purpose !== "turn") throw new UnauthenticatedError();
  return { auth: defaultEveAuth(ctx) };
}

export default eveChannel({
  auth: [
    (request: Request) => {
      // A missing or bad key throws, and a throwing walk rejects the request: fail closed.
      const key: KeyObject = coreKey();
      return mandateRouteAuth(request, key, Math.floor(Date.now() / 1000));
    },
  ],
  onMessage: mandateOnMessage,
  // Another person's message waits and runs as its own turn; never "retain" (docs/channels.md §4).
  turnPolicy: { mode: "steer", principal: "match" },
});
