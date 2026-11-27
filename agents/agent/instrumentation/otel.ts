import { tracing } from "#lib/telemetry.ts";

// Read at startup, before the server loads: no endpoint, no tracing.
export default tracing(process.env);
