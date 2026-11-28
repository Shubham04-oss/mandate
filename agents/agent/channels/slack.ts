// Slack behind the core (docs/channels.md §12): the core verifies Slack, maps the sender to a
// member and forwards with its assertion; eve's replies go out through the core's proxy.

import { defaultSlackAuth, defineSlackRenderer, slackChannel } from "eve/channels/slack";
import { channelProxyBase, verifyForward, withProxyKey, wrapChannel } from "#lib/forward.ts";

const core = channelProxyBase();

// eve renders no controls in v1: the core posts every approval card and settles every decision.
const noApprovalControls = defineSlackRenderer({
  events: { "interaction.opened": async () => {} },
});

export default wrapChannel(
  slackChannel({
    credentials: {
      // A selector the core's proxy maps to the installation; not a secret.
      botToken: ({ teamId }) => `xoxb-mandate-${teamId ?? ""}`,
      // Accepts only the core's forward assertion; eve never sees Slack's signing secret.
      webhookVerifier: (request, body) => verifyForward(request, body) !== null,
    },
    api: {
      apiBaseUrl: `${core}/private/v1/channels/slack/api/`,
      fileBaseUrl: `${core}/private/v1/channels/slack/files/`,
      fetch: withProxyKey(fetch),
    },
    // Another person's message waits and runs as its own turn; never "retain" (§4).
    turnPolicy: { mode: "steer", principal: "match" },
    // The sender's Slack identity; the wrapper adds the core's ids for the member it mapped.
    onAppMention: (ctx, message) => ({ auth: defaultSlackAuth(message, ctx) }),
    onDirectMessage: (ctx, message) => ({ auth: defaultSlackAuth(message, ctx) }),
    // The core forwards only threads it bound, so every forwarded follow-up is accepted.
    onMessage: (ctx, message) => ({ auth: defaultSlackAuth(message, ctx) }),
    // Only the core answers approvals.
    onAnswers: () => null,
    renderers: [noApprovalControls],
    uploadPolicy: { maxBytes: 10 * 1024 * 1024 },
  }),
  "slack",
);
