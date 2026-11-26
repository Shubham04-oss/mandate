import assert from "node:assert/strict";
import { test } from "node:test";
import { environment } from "../agent/sandbox.ts";
// eve's own accessor for an environment's provider (internal module).
import { getSandboxEnvironmentRuntime } from "../node_modules/eve/dist/src/shared/sandbox-environment.js";

const impl = getSandboxEnvironmentRuntime(environment).implementation;

test("the agent's sandbox file selects the no-capability provider", () => {
  assert.equal(environment.provider, "none");
  assert.equal(getSandboxEnvironmentRuntime(environment).providerName, "none");
});

test("the sandbox opens but refuses commands and file access", async () => {
  const { connection } = await impl.start({} as never, undefined, {});
  const io = {
    path: "/workspace/x",
    files: [],
    command: "id",
    env: {},
    workingDirectory: "/workspace",
  };
  await assert.rejects(connection.spawn(io), /no sandbox/);
  await assert.rejects(connection.readFile(io), /no sandbox/);
  await assert.rejects(connection.writeFiles(io), /no sandbox/);
  await assert.rejects(connection.removePath(io), /no sandbox/);
});

test("the build fails if workspace or skill files would need a real sandbox", async () => {
  assert.deepEqual(await impl.prepare({ isEmpty: true } as never), {});
  await assert.rejects(impl.prepare({ isEmpty: false } as never), /cannot hold/);
});
