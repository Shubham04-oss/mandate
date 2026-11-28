// The conversation registry's eve side (docs/channels.md §6.3): every session and turn on any
// channel is reported to the core, which answers a started one with the speaker's turn token.
import type { KeyObject } from "node:crypto";
import type { Assertion } from "./assertion.ts";
import { grantFrom, postCore, type TurnGrant } from "./state.ts";

export type RegistryEvent = "session.started" | "turn.started" | "turn.settled" | "session.ended";

export interface Report {
  event: RegistryEvent;
  session_id: string;
  channel?: string;
  continuation_token?: string;
  turn_id?: string;
  predecessor?: string;
}

export interface ReportDeps {
  base: string;
  key: KeyObject;
  fetch?: typeof fetch;
}

/**
 * Posts one report. A started event authenticates with the speaker's own token (forward or turn)
 * and returns their turn grant; later events use the held grant. Any failure throws, which fails
 * the turn before its first model call: no registry, no turn.
 */
export async function report(
  r: Report,
  speaker: { token: string | null; identity: Assertion | null },
  held: TurnGrant | null,
  deps: ReportDeps,
): Promise<TurnGrant | null> {
  const started = r.event === "session.started" || r.event === "turn.started";
  const bearer = started ? speaker.token : (held?.token ?? speaker.token);
  if (!bearer) throw new Error("no assertion to report this session with");
  const answer = await postCore(`${deps.base}/private/v1/sessions/report`, bearer, r, deps.fetch);
  return started ? grantFrom(answer, deps.key, speaker.identity?.member) : null;
}
