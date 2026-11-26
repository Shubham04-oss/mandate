// What eve asks the core. The core decides; this only makes approval cards appear early.
import type { Assertion } from "./assertion.ts";

/** The MCP connection's file name, so its tools arrive as `mandate__<tool>`. */
export const CONNECTION = "mandate";
const ASKS = "(asks for approval)";

export type Listing = Map<string, "run" | "ask">;

/** The core's MCP URL, read at call time; no fallback, so a missing setting fails loudly. */
export function coreMcpUrl(): string {
  const url = process.env.MANDATE_CORE_MCP_URL;
  if (!url) throw new Error("MANDATE_CORE_MCP_URL is not set");
  return url;
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
    result?: { tools?: { name?: unknown; description?: unknown }[] };
  };
  const tools = body.result?.tools;
  if (!Array.isArray(tools)) throw new Error("core tool listing is malformed");
  const out: Listing = new Map();
  for (const t of tools) {
    if (typeof t.name !== "string") continue;
    const ask = typeof t.description === "string" && t.description.endsWith(ASKS);
    out.set(t.name, ask ? "ask" : "run");
  }
  return out;
}

export type Status = "user-approval" | { type: "denied"; reason: string } | undefined;

export interface Caller {
  current: Assertion | null;
  initiator: Assertion | null;
  token: string | null;
}

/**
 * The agent-wide approval policy. Searching the catalog runs; anything else that is not a core
 * tool is denied; a core tool the core does not list is denied; an `ask` tool pauses for a person.
 * Any error denies.
 */
export async function approvalFor(
  toolName: string,
  caller: Caller,
  list: (token: string) => Promise<Listing>,
): Promise<Status> {
  const prefix = `${CONNECTION}__`;
  const search = toolName === "eve__search";
  if (!search && !toolName.startsWith(prefix)) {
    return { type: "denied", reason: "Only the core's tools are available." };
  }
  const { current, initiator, token } = caller;
  // A turn from another tenant or member than the one that started the session never runs.
  if (
    !current ||
    !initiator ||
    !token ||
    current.tenant !== initiator.tenant ||
    current.member !== initiator.member ||
    current.agent !== initiator.agent
  ) {
    return { type: "denied", reason: "This turn has no valid caller." };
  }
  // eve reaches connection tools only through this search. It only lists; each call it leads to
  // comes back here under its own name.
  if (search) return undefined;
  let listing: Listing;
  try {
    listing = await list(token);
  } catch {
    return { type: "denied", reason: "The core could not be reached." };
  }
  const decision = listing.get(toolName.slice(prefix.length));
  if (decision === undefined) return { type: "denied", reason: "This tool is not allowed." };
  return decision === "ask" ? "user-approval" : undefined;
}
