// The only tools eve has: the core's MCP server, called with the turn's assertion.
import { defineMcpClientConnection } from "eve/connections";
import { assertionOf, tokenOf } from "#lib/caller.ts";

export default defineMcpClientConnection({
  url: process.env.MANDATE_CORE_MCP_URL ?? "http://127.0.0.1:8080/mcp",
  description:
    "Mandate: the tools this person's agent may use (reading web pages, submitting forms). " +
    "The core decides every call; some calls wait for the owner's approval.",
  protocolVersionDiscovery: false,
  auth: (ctx) => ({
    principalType: "user",
    getToken: async () => {
      const current = ctx.session.auth.current;
      const token = tokenOf(current);
      const a = assertionOf(current);
      if (!token || !a) throw new Error("no assertion for this turn");
      return { token, expiresAt: a.expires_at * 1000 };
    },
  }),
});
