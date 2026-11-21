// Route auth: only the core reaches eve, and only with its signed assertion (tenant, member, agent).

import type { KeyObject } from "node:crypto";
import { extractBearerToken } from "eve/channels/auth";
import { eveChannel } from "eve/channels/eve";
import { loadPublicKey, verifyAssertion } from "#lib/assertion.ts";
import { AUTHENTICATOR } from "#lib/caller.ts";

// Loaded on first use: `eve build` evaluates this file without the runtime environment.
let key: KeyObject | undefined;

export default eveChannel({
  auth: [
    (request: Request) => {
      // A missing or bad key throws, and a throwing walk rejects the request: fail closed.
      key ??= loadPublicKey(process.env.MANDATE_ASSERTION_PUBLIC_KEY);
      const token = extractBearerToken(request.headers.get("authorization"));
      const a = token ? verifyAssertion(token, key, Math.floor(Date.now() / 1000)) : null;
      if (!token || !a) return null;
      return {
        authenticator: AUTHENTICATOR,
        issuer: "mandate",
        principalType: "user",
        principalId: `${a.tenant}:${a.member}`,
        // ponytail: the 5-minute assertion is kept in eve's session state so MCP calls can carry it.
        attributes: {
          tenant: a.tenant,
          member: a.member,
          agent: a.agent,
          expires_at: String(a.expires_at),
          assertion: token,
        },
      };
    },
  ],
});
