import { defineDynamic } from "eve/connections";
import { mandateConnection } from "#lib/connection.ts";

// Dynamic, so the URL is read from the environment at runtime: a static connection's URL is
// frozen into the build manifest, where the env var isn't set.
export default defineDynamic(mandateConnection);
