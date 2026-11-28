// The mandate connection, built per session so the URL and instance come from runtime state.
import type { KeyObject } from "node:crypto";
import { defineMcpClientConnection } from "eve/connections";
import type {
  ApprovalContext,
  ApprovalResponseContext,
  ApprovalResponseDecision,
} from "eve/tools/approval";
import { coreKey } from "./assertion.ts";
import { AUTHENTICATOR, identityOf, instanceKeyOf } from "./caller.ts";
import { approvalFor, coreMcpUrl, listTools, type Status } from "./core.ts";
import { currentGrant, MIN_LEFT_SECS } from "./state.ts";

interface Ctx {
  session: { id: string; auth: { current: Parameters<typeof identityOf>[0] } };
}

/** The agent's and the connection's request policy: the core's listing, for this turn's caller. */
export async function sessionApproval(
  ctx: Pick<ApprovalContext, "session" | "toolName" | "abortSignal">,
): Promise<Status> {
  try {
    const key = coreKey();
    const { session } = ctx;
    return await approvalFor(
      ctx.toolName,
      {
        current: identityOf(session.auth.current, key),
        initiator: identityOf(session.auth.initiator, key),
      },
      async () => listTools(coreMcpUrl(), (await currentGrant(session)).token, ctx.abortSignal),
    );
  } catch {
    return { type: "denied", reason: "This turn has no valid caller." };
  }
}

/**
 * Only the core settles an approval: an answer through eve's HTTP channel with a signed `answer`
 * token for the requester's tenant and agent. A typed "yes" or a channel button never does.
 */
export function answerOnly(
  ctx: Pick<ApprovalResponseContext, "request" | "response">,
  key: KeyObject,
): ApprovalResponseDecision {
  const responder = ctx.response.principal;
  const answer = responder.authenticator === AUTHENTICATOR ? identityOf(responder, key) : null;
  const requester = identityOf(ctx.request.principal, key);
  if (
    answer?.purpose === "answer" &&
    requester &&
    answer.tenant === requester.tenant &&
    answer.agent === requester.agent
  ) {
    return { status: "allowed" };
  }
  return { status: "rejected", reason: "Approvals are decided in Mandate." };
}

/** No verified caller, no connection; otherwise the core's MCP server for this tenant and agent. */
export function mandateConnection(ctx: Ctx) {
  const caller = identityOf(ctx.session.auth.current, coreKey());
  if (!caller) return null;
  return defineMcpClientConnection({
    url: coreMcpUrl(),
    // eve requires one for an authenticated dynamic connection: whose tools these are.
    instanceKey: instanceKeyOf(caller),
    description:
      "Mandate: the tools this person's agent may use (reading web pages, submitting forms). " +
      "The core decides every call; some calls wait for the owner's approval.",
    protocolVersionDiscovery: false,
    // Asks exactly where the agent's policy asks, so this connection's response policy applies.
    approval: { request: sessionApproval, response: (c) => answerOnly(c, coreKey()) },
    auth: (c) => ({
      principalType: "user",
      getToken: async () => {
        // The registry's turn token, renewed through the core when it has under 30 s left.
        const grant = await currentGrant(c.session);
        return { token: grant.token, expiresAt: (grant.expires_at - MIN_LEFT_SECS) * 1000 };
      },
    }),
  });
}
