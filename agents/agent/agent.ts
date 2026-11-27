import { defineAgent } from "eve";
import { defineDynamic } from "eve/models";
import { assertionOf, tokenOf } from "#lib/caller.ts";
import { approvalFor, listTools } from "#lib/core.ts";
import { gatewayModel, withMetadata } from "#lib/model.ts";
import { rememberCaller } from "#lib/posthog.ts";

function env(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`${name} is not set`);
  return v;
}

export default defineAgent({
  // Model ids and the gateway come from the environment: nothing here names a provider or model.
  // Runs before every model call, so each call's gateway log names its caller's verified ids.
  model: defineDynamic(({ session }) => {
    const caller = assertionOf(session.auth.current);
    const ids = { tenant: caller?.tenant, agent: caller?.agent, session: session.id };
    // Model-call events carry only the session id; this is where its caller's ids are known.
    rememberCaller(session.id, caller ?? {});
    return {
      model: gatewayModel(
        env("MANDATE_AI_GATEWAY_URL"),
        env("MANDATE_AI_GATEWAY_KEY"),
        env("MANDATE_MODEL_DEFAULT"),
        withMetadata(fetch, ids),
      ),
      modelContextWindowTokens: Number(process.env.MANDATE_MODEL_CONTEXT_TOKENS ?? 128000),
    };
  }),
  defaultTools: false,
  // Cards appear early; the core checks every call again at its MCP gate.
  approval: ({ session, toolName, abortSignal }) =>
    approvalFor(
      toolName,
      {
        current: assertionOf(session.auth.current),
        initiator: assertionOf(session.auth.initiator),
        token: tokenOf(session.auth.current),
      },
      (token) => listTools(env("MANDATE_CORE_MCP_URL"), token, abortSignal),
    ),
  experimental: {
    workflow: { world: "@workflow/world-postgres" },
  },
});
