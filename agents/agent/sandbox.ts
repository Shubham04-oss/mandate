import { defineSandbox } from "eve/sandbox";
import { NoSandbox } from "#lib/no-sandbox.ts";

// eve 1.3 always gives an agent a sandbox; select one that can do nothing instead of DefaultSandbox.
export const environment = NoSandbox.environment();
export default defineSandbox(() => environment.open());
