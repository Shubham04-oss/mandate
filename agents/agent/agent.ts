import { defineAgent } from "eve";
import { defineDynamic } from "eve/models";
import { assertionOf, tokenOf } from "#lib/caller.ts";
import { approvalFor, listTools } from "#lib/core.ts";
import { gatewayModel } from "#lib/model.ts";

function env(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`${name} is not set`);
  return v;
}

export default defineAgent({
  // Model ids and the gateway come from the environment: nothing here names a provider or model.
  model: defineDynamic(() => ({
    model: gatewayModel(
      env("MANDATE_AI_GATEWAY_URL"),
      env("MANDATE_AI_GATEWAY_KEY"),
      env("MANDATE_MODEL_DEFAULT"),
    ),
    modelContextWindowTokens: Number(process.env.MANDATE_MODEL_CONTEXT_TOKENS ?? 128000),
  })),
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
