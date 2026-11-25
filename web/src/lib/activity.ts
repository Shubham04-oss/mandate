import type { Activity, Approval } from "#/lib/api/index.ts";
import { type Clock, formatTime } from "#/lib/preferences.ts";

/** How a tool reads in a sentence, for what happened and for what was only tried. */
const phrases: Record<
	string,
	{ did: string; tried: string; asked: string; wants: string }
> = {
	"slack.reply": {
		did: "replied in Slack",
		tried: "tried to reply in Slack",
		asked: "asked to reply in Slack",
		wants: "Reply in Slack",
	},
	"web.read_page": {
		did: "read a page",
		tried: "tried to read a page",
		asked: "asked to read a page",
		wants: "Read a page",
	},
	"web.submit_form": {
		did: "submitted a form",
		tried: "tried to submit a form",
		asked: "asked to submit a form",
		wants: "Submit a form",
	},
};

const phrase = (tool: string) =>
	phrases[tool] ?? {
		did: `used ${tool}`,
		tried: `tried to use ${tool}`,
		asked: `asked to use ${tool}`,
		wants: `Use ${tool}`,
	};

/** An agent's name, or a plain one when the core only has its id (an unnamed or deleted agent). */
export function agentName(a: Activity["agent"]): string {
	if (a.name !== a.id) return a.name;
	return a.kind === "personal" ? "Personal agent" : "An agent";
}

/** How an ask ended, as a row's detail and mark. */
const asked: Record<
	NonNullable<Activity["outcome"]> | "unknown",
	{ detail: string; kind: ActivityRow["kind"] }
> = {
	approved: { detail: "Approved", kind: "approved" },
	denied: { detail: "Denied", kind: "refused" },
	expired: { detail: "Expired before anyone decided", kind: "refused" },
	pending: { detail: "Waiting for approval", kind: "waiting" },
	unknown: { detail: "Asked for approval", kind: "waiting" },
};

export type ActivityRow = {
	id: string;
	kind: "done" | "refused" | "approved" | "waiting";
	title: string;
	detail: string;
	when: string;
};

/** One activity entry as a row on Home's "Recent" panel, its time in the person's chosen format. */
export function activityRow(a: Activity, clock: Clock = "system"): ActivityRow {
	const p = phrase(a.tool);
	const name = agentName(a.agent);
	const when = formatTime(a.at, clock);
	if (a.decision === "refuse") {
		return {
			id: a.id,
			kind: "refused",
			title: `${name} ${p.tried}`,
			detail: a.reason ?? "Policy refused it",
			when,
		};
	}
	if (a.decision === "ask") {
		const ended = asked[a.outcome ?? "unknown"];
		return { id: a.id, title: `${name} ${p.asked}`, ...ended, when };
	}
	// A run that spent an approval says who gave it.
	return {
		id: a.id,
		kind: a.approved_by ? "approved" : "done",
		title: `${name} ${p.did}`,
		detail: a.approved_by
			? `Approved by ${a.approved_by.name}`
			: `For ${a.member.name}`,
		when,
	};
}

/** How many actions agents carried out today, refusals left out. */
export function handledToday(items: Activity[], now = new Date()): number {
	const day = now.toDateString();
	return items.filter(
		(a) => a.decision !== "refuse" && new Date(a.at).toDateString() === day,
	).length;
}

/** The host an approval's URL points at, when it has one. */
const host = (args: Record<string, unknown>) => {
	try {
		return typeof args.url === "string" ? new URL(args.url).host : null;
	} catch {
		return null;
	}
};

/** A short line for a waiting approval, e.g. "Submit a form on help.acme.com". */
export function describeApproval(a: Approval): string {
	if (a.tool === "slack.reply" && typeof a.args.channel === "string")
		return `Reply in ${a.args.channel}`;
	const where = host(a.args);
	const wants = phrase(a.tool).wants;
	return where ? `${wants} on ${where}` : wants;
}
