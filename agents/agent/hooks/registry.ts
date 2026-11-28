// Reports every session and turn to the core's registry; awaited, so a failed report fails the
// turn before its first model call (eve docs, guides/hooks.md, "Execution order").
import { defineHook, type HookContext } from "eve/hooks";
import { coreKey } from "#lib/assertion.ts";
import { identityOf, tokenOf } from "#lib/caller.ts";
import { corePrivateUrl } from "#lib/core.ts";
import { type RegistryEvent, report } from "#lib/registry.ts";
import { turnGrant } from "#lib/state.ts";

async function send(event: RegistryEvent, ctx: HookContext, turnId?: string): Promise<void> {
  const key = coreKey();
  const current = ctx.session.auth.current;
  const grant = await report(
    {
      event,
      session_id: ctx.session.id,
      // eve's own values: kind `channel:slack` or `http`, token `<channel>:<address>`.
      channel: ctx.channel.kind,
      continuation_token: ctx.channel.continuationToken,
      turn_id: turnId,
      predecessor: ctx.session.predecessor?.sessionId,
    },
    { token: tokenOf(current), identity: identityOf(current, key) },
    turnGrant.get(),
    { base: corePrivateUrl(), key },
  );
  if (grant) turnGrant.update(() => grant);
}

export default defineHook({
  events: {
    "session.started": (_event, ctx) => send("session.started", ctx),
    "turn.started": (event, ctx) => send("turn.started", ctx, event.data.turnId),
    "turn.settled": (event, ctx) => send("turn.settled", ctx, event.data.turnId),
    "session.ended": (_event, ctx) => send("session.ended", ctx),
  },
});
