import { posthog } from "#lib/posthog.ts";

// Read at startup: no PostHog key and host, no events.
export default posthog(process.env);
