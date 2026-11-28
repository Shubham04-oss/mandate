import assert from "node:assert/strict";
import { test } from "node:test";
import { mandateConnection } from "../agent/lib/connection.ts";
// eve's own runtime check for a dynamic connection's value (internal, so untyped).
// @ts-expect-error -- no declaration file for this internal module
import { resolveDynamicConnectionValue } from "../node_modules/eve/dist/src/runtime/resolve-connection.js";
import { forger, PEM, turn } from "./tokens.ts";

process.env.MANDATE_ASSERTION_PUBLIC_KEY = PEM;

const at = {
  connectionName: "mandate",
  logicalPath: "connections/mandate.ts",
  sourceId: "connections/mandate.ts",
};

const session = (attributes: Record<string, string>) => ({
  session: { id: "wrun_1", auth: { current: { attributes } } },
});

test("with a verified caller, the connection passes eve's runtime check", () => {
  process.env.MANDATE_CORE_MCP_URL = "http://core.internal:8080/mcp";
  const value = mandateConnection(session({ assertion: turn() }));
  assert.ok(value);
  const r = resolveDynamicConnectionValue(value, at);
  assert.equal(r.url, "http://core.internal:8080/mcp");
  assert.equal(r.connectionName, "mandate");
});

test("without a verified caller there is no connection", () => {
  assert.equal(mandateConnection({ session: { id: "wrun_1", auth: { current: null } } }), null);
  // Planted attributes or a forged token are no caller.
  const planted = { tenant: "acme", member: "ann", agent: "ann-op", expires_at: "2000000000" };
  assert.equal(mandateConnection(session(planted)), null);
  assert.equal(mandateConnection(session({ assertion: turn({}, forger.privateKey) })), null);
});
