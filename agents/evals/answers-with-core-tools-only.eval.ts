// answers_with_core_tools_only: the agent answers through the core's tools and nothing else.
// Needs a model through the gateway; skips cleanly without one.
import { defineEval } from "eve/evals";

export default defineEval({
  async test(t) {
    if (!process.env.MANDATE_AI_GATEWAY_URL || !process.env.MANDATE_MODEL_DEFAULT) {
      t.skip("MANDATE_AI_GATEWAY_URL or MANDATE_MODEL_DEFAULT not set");
      return;
    }
    const session = await t.session({
      stubs: [
        {
          id: "read",
          tool: "mandate__web.read_page",
          outcome: {
            response: { title: "Release notes", text: "Version 4.2 ships on Friday." },
          },
        },
      ],
    });
    const turn = await session.send("Read https://docs.test/notes and tell me when 4.2 ships.");
    turn.expectOk();
    turn.calledTool("mandate__web.read_page", { count: 1 });
    turn.messageIncludes("Friday");
  },
});
