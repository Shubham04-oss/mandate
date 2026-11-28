// `POST /mandate/v1/sessions` (docs/channels.md §6.4): the core starts a session on a channel,
// for a handoff, with history the core resolved from its own conversation record.

import { defineChannel, POST, type RouteHandlerArgs } from "eve/channels";
import { extractBearerToken } from "eve/channels/auth";
import { z } from "zod";
import type { Assertion } from "#lib/assertion.ts";
import { AUTHENTICATOR, type ChannelAttr, mandateAttributes } from "#lib/caller.ts";
import { verifyForward } from "#lib/forward.ts";
import slack from "./slack.ts";

export const SESSIONS_ROUTE = "/mandate/v1/sessions";

const text = z
  .string()
  .min(1)
  .max(32 * 1024);
const Body = z.strictObject({
  target: z.discriminatedUnion("kind", [
    z.strictObject({ kind: z.literal("web") }),
    z.strictObject({
      kind: z.literal("slack"),
      teamId: z.string().min(1).max(128),
      channelId: z.string().min(1).max(128),
      threadTs: z.string().min(1).max(64).optional(),
    }),
    z.strictObject({ kind: z.enum(["whatsapp", "sms"]), phoneNumber: z.string().min(1).max(32) }),
  ]),
  message: text.optional(),
  history: z.array(z.strictObject({ role: z.enum(["user", "assistant"]), text })).max(50),
  idempotencyKey: z.string().min(1).max(256),
});

function authFor(a: Assertion, token: string, channel: ChannelAttr) {
  return {
    authenticator: AUTHENTICATOR,
    issuer: "mandate",
    principalType: "user",
    principalId: `${a.tenant}:${a.member}`,
    attributes: mandateAttributes({}, a, token, channel),
  };
}

const bad = (error: string) => Response.json({ error }, { status: 400 });

/** Starts the session the core asked for; 401 without a forward token that names a thread. */
export async function startSession(
  request: Request,
  { from, to }: Pick<RouteHandlerArgs, "from" | "to">,
): Promise<Response> {
  const raw = new Uint8Array(await request.arrayBuffer());
  const token = extractBearerToken(request.headers.get("authorization"));
  const a = token ? verifyForward(request, raw) : null;
  if (!token || !a || a.thread === undefined || a.member === undefined) {
    return new Response("unauthorized", { status: 401 });
  }
  let parsed: z.infer<typeof Body>;
  try {
    parsed = Body.parse(JSON.parse(new TextDecoder().decode(raw)));
  } catch {
    return bad("invalid history");
  }
  const { target, message, history, idempotencyKey } = parsed;
  if (target.kind === "web") {
    if (a.conversation === undefined) return bad("no conversation");
    const auth = authFor(a, token, "web");
    const { session } = await from(a.conversation).create({ auth, history });
    if (message !== undefined) await session.send(message, { auth, idempotencyKey });
    return Response.json({ sessionId: session.id }, { status: 202 });
  }
  // ponytail: WhatsApp and SMS targets land with stream E's channel file.
  if (target.kind !== "slack") return bad("channel not available");
  if (message === undefined) return bad("a channel session needs a message");
  const delivery = await to(slack, {
    channelId: target.channelId,
    installationTeamId: target.teamId,
    ...(target.threadTs !== undefined && { threadTs: target.threadTs }),
  }).send(message, { auth: authFor(a, token, "slack"), history, idempotencyKey });
  return Response.json({ sessionId: delivery.sessionId }, { status: 202 });
}

export default defineChannel({
  routes: [POST(SESSIONS_ROUTE, async (request, args) => startSession(request, args))],
  turnPolicy: { mode: "steer", principal: "match" },
});
