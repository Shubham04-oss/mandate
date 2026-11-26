// The mandate connection, built per session so the URL and instance come from runtime state.
import { defineMcpClientConnection } from "eve/connections";
import { assertionOf, instanceKeyOf, tokenOf } from "./caller.ts";
import { coreMcpUrl } from "./core.ts";

interface Ctx {
  session: { auth: { current: Parameters<typeof assertionOf>[0] } };
}

/** No verified caller, no connection; otherwise the core's MCP server for this tenant and agent. */
export function mandateConnection(ctx: Ctx) {
  const caller = assertionOf(ctx.session.auth.current);
  if (!caller) return null;
  return defineMcpClientConnection({
    url: coreMcpUrl(),
    // eve requires one for an authenticated dynamic connection: whose tools these are.
    instanceKey: instanceKeyOf(caller),
    description:
      "Mandate: the tools this person's agent may use (reading web pages, submitting forms). " +
      "The core decides every call; some calls wait for the owner's approval.",
    protocolVersionDiscovery: false,
    auth: (c) => ({
      principalType: "user",
      getToken: async () => {
        const current = c.session.auth.current;
        const token = tokenOf(current);
        const a = assertionOf(current);
        if (!token || !a) throw new Error("no assertion for this turn");
        return { token, expiresAt: a.expires_at * 1000 };
      },
    }),
  });
}
