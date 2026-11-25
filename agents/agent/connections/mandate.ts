import { defineDynamic, defineMcpClientConnection } from "eve/connections";
import { assertionOf, tokenOf } from "#lib/caller.ts";
import { coreMcpUrl } from "#lib/core.ts";

// Dynamic, so the URL is read from the environment at runtime: a static connection's URL is
// frozen into the build manifest, where the env var isn't set.
export default defineDynamic(() =>
  defineMcpClientConnection({
    url: coreMcpUrl(),
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
  }),
);
