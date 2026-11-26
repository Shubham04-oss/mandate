// A sandbox provider with no capability: the agent is untrusted, so it gets no shell, files or code execution.
import { defineSandboxProvider, type SandboxConnection } from "eve/sandbox/provider";

const refuse = async (): Promise<never> => {
  throw new Error("this agent has no sandbox");
};

const connection: SandboxConnection = {
  spawn: refuse,
  readFile: refuse,
  writeFiles: refuse,
  removePath: refuse,
  stop: async () => {},
};

/** Opens without error (so eve's per-session prewarm stays quiet) and refuses every operation. */
export const NoSandbox = defineSandboxProvider({
  name: "none",
  environment() {
    return {
      async prepare(ctx) {
        // Workspace or skill files would need a real sandbox; fail the build rather than drop them.
        if (!ctx.isEmpty) throw new Error("NoSandbox cannot hold workspace or skill files");
        return {};
      },
      async start() {
        return { connection, state: {} };
      },
      async resume() {
        return connection;
      },
      async onSessionEnd() {},
    };
  },
});
