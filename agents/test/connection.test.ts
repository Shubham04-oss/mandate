import assert from "node:assert/strict";
import { test } from "node:test";
import { mandateConnection } from "../agent/lib/connection.ts";
// eve's own runtime check for a dynamic connection's value (internal, so untyped).
// @ts-expect-error -- no declaration file for this internal module
import { resolveDynamicConnectionValue } from "../node_modules/eve/dist/src/runtime/resolve-connection.js";

const at = {
  connectionName: "mandate",
  logicalPath: "connections/mandate.ts",
  sourceId: "connections/mandate.ts",
};
const attributes = {
  tenant: "acme",
  member: "ann",
  agent: "ann-op",
  expires_at: "2000000000",
  assertion: "tok",
};

test("with a verified caller, the connection passes eve's runtime check", () => {
  process.env.MANDATE_CORE_MCP_URL = "http://core.internal:8080/mcp";
  const value = mandateConnection({ session: { auth: { current: { attributes } } } });
  assert.ok(value);
  const r = resolveDynamicConnectionValue(value, at);
  assert.equal(r.url, "http://core.internal:8080/mcp");
  assert.equal(r.connectionName, "mandate");
});

test("without a verified caller there is no connection", () => {
  assert.equal(mandateConnection({ session: { auth: { current: null } } }), null);
});
