// What eve asks the core. The core decides; this only makes approval cards appear early.
import type { Assertion } from "./assertion.ts";

/** The MCP connection's file name, so its tools arrive as `mandate__<tool>`. */
export const CONNECTION = "mandate";
/** Where the core's listing puts each tool's decision (docs/channels.md §11, `mcp.rs`). */
const DECISION = "mandate/decision";

export type Listing = Map<string, "run" | "ask">;

/** The core's MCP URL, read at call time; no fallback, so a missing setting fails loudly. */
export function coreMcpUrl(): string {
  const url = process.env.MANDATE_CORE_MCP_URL;
  if (!url) throw new Error("MANDATE_CORE_MCP_URL is not set");
  return url;
}

/** The core's private base URL (registry, renewal, channel proxy); never through the tunnel. */
export function corePrivateUrl(env: NodeJS.ProcessEnv = process.env): string {
  const url = env.MANDATE_CORE_PRIVATE_URL;
  if (!url) throw new Error("MANDATE_CORE_PRIVATE_URL is not set");
  return url.replace(/\/+$/, "");
}

/** The tools the core lists for this assertion, each tagged run or ask; refused tools are absent. */
export async function listTools(
  url: string,
  token: string,
  signal?: AbortSignal,
  fetchFn: typeof fetch = fetch,
): Promise<Listing> {
  const res = await fetchFn(url, {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      "content-type": "application/json",
      accept: "application/json, text/event-stream",
    },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list", params: {} }),
    signal,
  });
  if (!res.ok) throw new Error(`core tool listing answered ${res.status}`);
  const body = (await res.json()) as {
    result?: { tools?: { name?: unknown; _meta?: Record<string, unknown> }[] };
  };
  const tools = body.result?.tools;
  if (!Array.isArray(tools)) throw new Error("core tool listing is malformed");
  const out: Listing = new Map();
  for (const t of tools) {
    const decision = t._meta?.[DECISION];
    // A tool without a decision this code knows is left out, so it is denied.
    if (typeof t.name !== "string" || (decision !== "run" && decision !== "ask")) continue;
    out.set(t.name, decision);
  }
  return out;
}

export type Status = "user-approval" | { type: "denied"; reason: string } | undefined;

export interface Caller {
  current: Assertion | null;
  initiator: Assertion | null;
}

/**
 * The agent-wide approval policy. Searching the catalog runs; anything else that is not a core
 * tool is denied; a core tool the core does not list is denied; an `ask` tool pauses for a person.
 * Any error denies. `list` fetches the listing with the turn's token.
 */
export async function approvalFor(
  toolName: string,
  caller: Caller,
  list: () => Promise<Listing>,
): Promise<Status> {
  const prefix = `${CONNECTION}__`;
  const search = toolName === "eve__search";
  if (!search && !toolName.startsWith(prefix)) {
    return { type: "denied", reason: "Only the core's tools are available." };
  }
  const { current, initiator } = caller;
  // Multiplayer: any member of the session's tenant may speak to its agent, and the core's gate
  // decides each call with that speaker. Another tenant or agent, or no member, never runs.
  if (
    !current ||
    !initiator ||
    current.member === undefined ||
    current.tenant !== initiator.tenant ||
    current.agent !== initiator.agent
  ) {
    return { type: "denied", reason: "This turn has no valid caller." };
  }
  // eve reaches connection tools only through this search. It only lists; each call it leads to
  // comes back here under its own name.
  if (search) return undefined;
  let listing: Listing;
  try {
    listing = await list();
  } catch {
    return { type: "denied", reason: "The core could not be reached." };
  }
  const decision = listing.get(toolName.slice(prefix.length));
  if (decision === undefined) return { type: "denied", reason: "This tool is not allowed." };
  return decision === "ask" ? "user-approval" : undefined;
}
