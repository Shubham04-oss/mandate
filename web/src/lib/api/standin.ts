/**
 * ponytail: STAND-IN RESPONSES, not the core. They answer every call in the contract's shapes
 * (section 4 of the v1-remaining design) so the screens can be built and reviewed before the core
 * serves these endpoints. Setting VITE_MANDATE_API_URL bypasses this file entirely.
 *
 * They return real `Response` objects, so the client's response handling runs exactly as it will
 * against the core. The people, organizations and activity are the same made-up sample data the
 * screens already use (see web/README.md).
 *
 * To reach every sign-in step, the email before the @ picks the outcome: mfa@, setup@, orgs@,
 * verify@, sso@, method@ and noaccess@; member@ signs in as someone who isn't an admin; any other
 * email signs straight in. The code 000000 is always wrong, the password "wrong" never matches, and
 * the reset or invite token "expired" has expired.
 *
 * Conversations: an ask reads a page and answers; one that mentions a form or filling something in
 * then waits for approval; one with "fail" in it fails as the core does when the agent can't start.
 */
import type {
	Activity,
	Agent,
	Approval,
	ApprovalState,
	Conversation,
	ConversationEvent,
	Grant,
	Me,
	Method,
	Org,
	Person,
	SignInOutcome,
	Team,
} from "#/lib/api/index.ts";
import type { EventSourceLike } from "#/lib/conversation-stream.ts";

const json = (status: number, body?: unknown) =>
	new Response(body === undefined ? null : JSON.stringify(body), {
		status,
		headers: { "Content-Type": "application/json" },
	});

const fail = (
	status: number,
	code: string,
	message: string,
	headers?: Record<string, string>,
) =>
	new Response(JSON.stringify({ error: { code, message } }), {
		status,
		headers: { "Content-Type": "application/json", ...headers },
	});

// ── Sample data in contract shapes ───────────────────────────────────────────────────────────────

const person = (
	id: string,
	name: string,
	email: string,
	avatar?: string,
): Person => ({
	id,
	name,
	email,
	avatar_url: avatar,
});

const ada = person(
	"mem_ada",
	"Ada Brennan",
	"ada@northstar.io",
	"/avatars/ada.svg",
);
const priya = person(
	"mem_priya",
	"Priya Shah",
	"priya@northstar.io",
	"/avatars/priya.svg",
);
const rohan = person(
	"mem_rohan",
	"Rohan Mehta",
	"rohan@northstar.io",
	"/avatars/rohan.svg",
);
const raya = person(
	"mem_raya",
	"Raya Patel",
	"raya@northstar.io",
	"/avatars/raya.svg",
);
const kenji = person(
	"mem_kenji",
	"Kenji Sato",
	"kenji@northstar.io",
	"/avatars/member.svg",
);
const people = [ada, priya, rohan, raya, kenji];

const northstar: Org = {
	id: "org_northstar",
	name: "Northstar",
	summary: { people: 12, agents: 9 },
};
const orgs: Org[] = [
	northstar,
	{
		id: "org_harbourline",
		name: "Harbourline",
		summary: { people: 48, agents: 21 },
	},
	{
		id: "org_tidewater",
		name: "Tidewater Labs",
		summary: { people: 6, agents: 3 },
	},
];

const teams: Team[] = [
	{ id: "team_platform", name: "Platform", members: 8, is_admin: true },
	{ id: "team_support", name: "Support", members: 5, is_admin: false },
	{ id: "team_finance", name: "Finance", members: 4, is_admin: false },
];

const yourAgent: Agent = {
	id: "agt_ada",
	name: "Your agent",
	kind: "personal",
	owner: ada,
	enabled: true,
};
const supportBot: Agent = {
	id: "agt_support",
	name: "Support bot",
	kind: "team",
	owner: { id: "team_support", name: "Support" },
	enabled: true,
};
const opsBot: Agent = {
	id: "agt_ops",
	name: "Ops bot",
	kind: "team",
	owner: { id: "team_platform", name: "Platform" },
	enabled: true,
};
const salesBot: Agent = {
	id: "agt_sales",
	name: "Sales bot",
	kind: "team",
	owner: { id: "team_finance", name: "Finance" },
	enabled: true,
};

const me = (org: Org = northstar): Me => ({
	person: ada,
	org,
	orgs,
	mfa_enabled: true,
	is_admin: !isMember(),
	teams: [
		{ id: "team_platform", name: "Platform" },
		{ id: "team_finance", name: "Finance" },
	],
});

/** Today at hh:mm local time, as RFC 3339. */
const today = (hh: number, mm: number) => {
	const d = new Date();
	d.setHours(hh, mm, 0, 0);
	return d.toISOString();
};
const inMinutes = (m: number) =>
	new Date(Date.now() + m * 60_000).toISOString();
const minutesAgo = (m: number) =>
	new Date(Date.now() - m * 60_000).toISOString();

const approvals: Approval[] = [
	{
		id: "apv_form",
		agent: supportBot,
		tool: "web.submit_form",
		args: { url: "https://help.acme.com/tickets/4821", action: "Close ticket" },
		effect: "write",
		requested_by: priya,
		requested_at: minutesAgo(6),
		expires_at: inMinutes(54),
		state: "pending",
		can_decide: true,
	},
	{
		id: "apv_reply",
		agent: salesBot,
		tool: "slack.reply",
		args: {
			channel: "#deals",
			text: "The Q3 discount for Northwind is approved at 12%.",
		},
		effect: "write",
		requested_by: raya,
		requested_at: minutesAgo(9),
		expires_at: inMinutes(51),
		state: "pending",
		can_decide: true,
	},
	{
		id: "apv_read",
		agent: opsBot,
		tool: "web.read_page",
		args: { url: "https://grafana.northstar.io/d/incidents" },
		effect: "read",
		requested_by: rohan,
		requested_at: minutesAgo(12),
		expires_at: inMinutes(48),
		state: "pending",
		can_decide: true,
	},
];

const activity: Activity[] = [
	{
		id: "act_1",
		at: today(10, 12),
		agent: yourAgent,
		member: ada,
		tool: "slack.reply",
		decision: "run",
	},
	{
		id: "act_2",
		at: today(9, 40),
		agent: salesBot,
		member: raya,
		tool: "web.submit_form",
		decision: "refuse",
		reason: "Policy doesn't allow form submits on crm.northstar.io",
	},
	{
		id: "act_3",
		at: today(9, 5),
		agent: supportBot,
		member: priya,
		tool: "web.submit_form",
		decision: "ask",
		approval_id: "apv_old",
		approved_by: raya,
	},
	{
		id: "act_4",
		at: today(8, 52),
		agent: yourAgent,
		member: ada,
		tool: "web.read_page",
		decision: "run",
	},
	{
		id: "act_5",
		at: today(8, 30),
		agent: opsBot,
		member: rohan,
		tool: "web.read_page",
		decision: "run",
	},
	{
		id: "act_6",
		at: today(8, 10),
		agent: yourAgent,
		member: ada,
		tool: "slack.reply",
		decision: "run",
	},
	{
		id: "act_7",
		at: today(7, 45),
		agent: supportBot,
		member: kenji,
		tool: "slack.reply",
		decision: "run",
	},
	{
		id: "act_8",
		at: today(7, 20),
		agent: yourAgent,
		member: ada,
		tool: "web.read_page",
		decision: "run",
	},
];

// ── Session (per tab, so a reload keeps you signed in) ───────────────────────────────────────────

const sessionKey = "mandate.standin.session";
const store = () =>
	typeof window === "undefined" ? null : window.sessionStorage;
// The session remembers which organization it signed in to, as the core's session does.
const sessionOrg = (): Org | null => {
	const id = store()?.getItem(sessionKey);
	return id ? (orgs.find((o) => o.id === id) ?? northstar) : null;
};
const signedIn = () => sessionOrg() !== null;
const memberKey = "mandate.standin.member";
const isMember = () => store()?.getItem(memberKey) === "1";
const setSession = (org: Org | null) => {
	if (org) {
		store()?.setItem(sessionKey, org.id);
	} else {
		store()?.removeItem(sessionKey);
		store()?.removeItem(memberKey);
	}
};

const signIn = (org: Org = northstar): Response => {
	setSession(org);
	return json(200, {
		status: "signed_in",
		me: me(org),
	} satisfies SignInOutcome);
};

let pendingCount = 0;
const next = (
	step: Extract<SignInOutcome, { status: "next" }>["next"],
	extra?: object,
) =>
	json(200, {
		status: "next",
		next: step,
		pending: `pnd_standin_${++pendingCount}`,
		...extra,
	});

/** What signing in with this email leads to; see the file comment. */
const outcomeFor = (email: string): Response => {
	const hint = email.split("@")[0]?.toLowerCase();
	switch (hint) {
		case "mfa":
			return next("mfa");
		case "setup":
			return next("mfa_setup");
		case "orgs":
			return next("choose_org", { orgs });
		case "verify":
			return next("verify_email");
		case "sso":
			return next("sso_required", { redirect_url: "/sign-in?standin=sso" });
		case "method":
			return next("method_required", { methods: ["sso", "magic_auth"] });
		case "member":
			store()?.setItem(memberKey, "1");
			return signIn();
		case "noaccess":
			return fail(
				403,
				"no_access",
				"This account isn't part of an organization on oryxa yet.",
			);
		default:
			return signIn();
	}
};

const validEmail = (v: unknown): v is string =>
	typeof v === "string" && /^[^@\s]+@[^@\s]+$/.test(v);
const filled = (v: unknown) => typeof v === "string" && v.trim() !== "";
const badEmail = () => fail(400, "invalid_input", "Enter a valid email.");
const wrongCode = () =>
	fail(400, "code_invalid", "That code isn't right. Check it and try again.");
const accepted = () => json(202);
const signedOut = () => fail(401, "forbidden", "Sign in to continue.");

// ── Conversations, kept for this page load ───────────────────────────────────────────────────────

type Stored = { conversation: Conversation; events: ConversationEvent[] };
const conversations = new Map<string, Stored>();
const watchers = new Map<string, Set<(e: ConversationEvent) => void>>();

/** Strips the id and kind off an event, leaving what the core puts in `data`. */
const dataOf = (e: ConversationEvent): Record<string, unknown> => {
	const { id: _id, kind: _kind, ...data } = e;
	return data;
};

type EventBody = ConversationEvent extends infer E
	? E extends ConversationEvent
		? Omit<E, "id">
		: never
	: never;

function push(id: string, body: EventBody) {
	const c = conversations.get(id);
	if (!c) return;
	const event = { ...body, id: c.events.length + 1 } as ConversationEvent;
	c.events.push(event);
	c.conversation.updated_at = new Date().toISOString();
	for (const fn of watchers.get(id) ?? []) fn(event);
}

const setState = (id: string, state: Conversation["state"]) => {
	const c = conversations.get(id);
	if (c) c.conversation.state = state;
};

/** Runs steps one after another, each `ms` apart, like a turn arriving over a stream. */
function later(steps: (() => void)[], ms = 700) {
	steps.forEach((step, i) => {
		setTimeout(step, ms * (i + 1));
	});
}

function newConversation(
	message: string,
	state: Conversation["state"],
): string {
	const id = `cnv_${Math.random().toString(36).slice(2, 10)}`;
	const at = new Date().toISOString();
	conversations.set(id, {
		conversation: {
			id,
			title: message.slice(0, 80),
			state,
			created_at: at,
			updated_at: at,
		},
		events: [],
	});
	push(id, { kind: "message", role: "user", text: message });
	return id;
}

/** A turn: read a page, answer, and when asked to fill something in, wait for approval. */
function turn(id: string, message: string) {
	setState(id, "working");
	const wantsForm = /form|fill|submit|book|request/i.test(message);
	later([
		() =>
			push(id, { kind: "tool", name: "web.read_page", status: "requested" }),
		() => push(id, { kind: "tool", name: "web.read_page", status: "running" }),
		() =>
			push(id, { kind: "tool", name: "web.read_page", status: "completed" }),
		() =>
			push(id, {
				kind: "message",
				role: "assistant",
				text: wantsForm
					? "The Team plan is $12 per seat a month, billed yearly, so 25 seats come to $3,600 a year.\n\nI've filled in the upgrade request. Sending it changes Northstar's plan, so it needs your OK."
					: "Here's what the page says: the new refund policy gives 30 days for a full refund on yearly plans, and 14 days on monthly ones. Details are at https://help.acme.com/refunds.",
			}),
		() => {
			if (!wantsForm) {
				push(id, { kind: "done" });
				setState(id, "idle");
				return;
			}
			push(id, { kind: "tool", name: "web.submit_form", status: "requested" });
			const approval: Approval = {
				id: `apv_${id}`,
				agent: yourAgent,
				tool: "web.submit_form",
				args: {
					url: "https://help.acme.com/upgrade",
					company: "Northstar",
					plan: "Team, billed yearly",
					seats: 25,
				},
				effect: "external",
				requested_by: ada,
				requested_at: new Date().toISOString(),
				expires_at: inMinutes(30),
				state: "pending",
				can_decide: true,
			};
			approvals.unshift(approval);
			setState(id, "waiting");
			push(id, { kind: "approval_wait", approval });
		},
	]);
}

/** The core resumes a waiting turn once its approval is decided. */
function resume(approval: Approval) {
	const id = approval.id.replace(/^apv_/, "");
	if (!conversations.has(id)) return;
	const state: ApprovalState = approval.state;
	const approved = state === "approved";
	setState(id, "working");
	later([
		() => push(id, { kind: "approval_done", approval_id: approval.id, state }),
		() =>
			push(id, {
				kind: "tool",
				name: "web.submit_form",
				status: approved ? "running" : "denied",
			}),
		...(approved
			? [
					() =>
						push(id, {
							kind: "tool",
							name: "web.submit_form",
							status: "completed",
						}),
				]
			: []),
		() =>
			push(id, {
				kind: "message",
				role: "assistant",
				text: approved
					? "Sent. Acme says the request is in and someone will confirm by email within a day."
					: "Okay, I won't send it. The form is still filled in if you change your mind.",
			}),
		() => {
			push(id, { kind: "done" });
			setState(id, "idle");
		},
	]);
}

// An earlier conversation, so the list has something in it.
{
	const id = newConversation("Summarise the new refund policy page", "idle");
	push(id, { kind: "tool", name: "web.read_page", status: "requested" });
	push(id, { kind: "tool", name: "web.read_page", status: "completed" });
	push(id, {
		kind: "message",
		role: "assistant",
		text: "The new refund policy gives 30 days for a full refund on yearly plans, and 14 days on monthly ones.",
	});
	push(id, { kind: "done" });
	const c = conversations.get(id);
	if (c) {
		c.conversation.created_at = minutesAgo(90);
		c.conversation.updated_at = minutesAgo(88);
	}
}

/**
 * Stands in for the browser's EventSource on a conversation: replays the log from the start, as
 * the core does for a stream opened without Last-Event-ID, then sends new events as they come.
 */
export function standInEvents(id: string): EventSourceLike {
	const handlers = new Map<
		string,
		((e: { data: string; lastEventId: string }) => void)[]
	>();
	const send = (e: ConversationEvent) => {
		for (const fn of handlers.get(e.kind) ?? [])
			fn({ data: JSON.stringify(dataOf(e)), lastEventId: String(e.id) });
	};
	const source: EventSourceLike = {
		readyState: 0,
		onopen: null,
		onerror: null,
		addEventListener: (kind, fn) => {
			handlers.set(kind, [...(handlers.get(kind) ?? []), fn]);
		},
		close: () => {
			source.readyState = 2;
			watchers.get(id)?.delete(send);
		},
	};
	setTimeout(() => {
		if (source.readyState === 2) return;
		const c = conversations.get(id);
		if (!c) {
			source.readyState = 2;
			source.onerror?.();
			return;
		}
		source.readyState = 1;
		source.onopen?.();
		for (const e of c.events) send(e);
		watchers.set(id, (watchers.get(id) ?? new Set()).add(send));
	}, 150);
	return source;
}

// ── Grants ───────────────────────────────────────────────────────────────────────────────────────

const daysFromNow = (d: number) =>
	new Date(Date.now() + d * 86_400_000).toISOString();
const grants: Grant[] = [
	{
		id: "grt_1",
		principal: { kind: "team", id: "team_platform" },
		tool: "web.read_page",
		standing: false,
		expires_at: daysFromNow(60),
		state: "active",
		granted_by: ada,
		reason: null,
	},
	{
		id: "grt_2",
		principal: { kind: "member", id: "mem_rohan" },
		tool: "web.read_page",
		standing: false,
		expires_at: daysFromNow(21),
		state: "active",
		granted_by: ada,
		reason: null,
	},
	{
		id: "grt_3",
		principal: { kind: "agent", id: "agt_ops" },
		tool: "web.submit_form",
		standing: false,
		expires_at: daysFromNow(10),
		state: "active",
		granted_by: ada,
		reason: "Weekly vendor forms",
	},
	{
		id: "grt_4",
		principal: { kind: "member", id: "mem_priya" },
		tool: "web.submit_form",
		standing: false,
		expires_at: daysFromNow(-3),
		state: "expired",
		granted_by: ada,
		reason: null,
	},
];
const catalog = ["web.read_page", "web.submit_form"];
const principalExists = (kind: unknown, id: unknown) =>
	(kind === "member" && people.some((p) => p.id === id)) ||
	(kind === "team" && teams.some((t) => t.id === id)) ||
	(kind === "agent" &&
		[yourAgent, supportBot, opsBot, salesBot].some((a) => a.id === id));
const forbidden = () =>
	fail(403, "forbidden", "Only your organization's admins can do this.");

type Body = Record<string, unknown>;
type Handler = (
	match: RegExpMatchArray,
	body: Body,
) => Response | Promise<Response>;

const routes: [Method, RegExp, Handler][] = [
	[
		"POST",
		/^\/v1\/auth\/code\/start$/,
		(_, b) => (validEmail(b.email) ? accepted() : badEmail()),
	],
	[
		"POST",
		/^\/v1\/auth\/code\/verify$/,
		(_, b) =>
			!validEmail(b.email)
				? badEmail()
				: b.code === "000000"
					? wrongCode()
					: outcomeFor(b.email),
	],
	[
		"POST",
		/^\/v1\/auth\/password$/,
		(_, b) =>
			!validEmail(b.email)
				? badEmail()
				: b.password === "wrong"
					? fail(
							401,
							"invalid_credentials",
							"That email and password don't match.",
						)
					: outcomeFor(b.email),
	],
	[
		"POST",
		/^\/v1\/auth\/sign-up$/,
		(_, b) =>
			!validEmail(b.email)
				? badEmail()
				: !filled(b.name) || !filled(b.org_name)
					? fail(
							400,
							"invalid_input",
							"Enter your name and your organization's name.",
						)
					: next("verify_email"),
	],
	[
		"POST",
		/^\/v1\/auth\/verify-email$/,
		(_, b) => (b.code === "000000" ? wrongCode() : signIn()),
	],
	["POST", /^\/v1\/auth\/verify-email\/resend$/, () => accepted()],
	[
		"POST",
		/^\/v1\/auth\/mfa\/verify$/,
		(_, b) => (b.code === "000000" ? wrongCode() : signIn()),
	],
	[
		"POST",
		/^\/v1\/auth\/mfa\/setup$/,
		async () => {
			const qr = await fetch("/sample/mfa-qr.svg")
				.then((r) => r.text())
				.catch(() => "");
			return json(200, {
				qr_svg: qr,
				secret: "JBSWY3DPEHPK3PXP",
				challenge: "chl_standin",
			});
		},
	],
	[
		"POST",
		/^\/v1\/auth\/mfa\/enable$/,
		(_, b) =>
			b.code === "000000" ? wrongCode() : b.pending ? signIn() : json(204),
	],
	[
		"POST",
		/^\/v1\/auth\/choose-org$/,
		(_, b) => {
			const org = orgs.find((o) => o.id === b.org);
			return org
				? signIn(org)
				: fail(400, "invalid_input", "Choose one of your organizations.");
		},
	],
	[
		"POST",
		/^\/v1\/auth\/sso\/start$/,
		(_, b) => {
			if (!validEmail(b.email)) return badEmail();
			if (!b.email.endsWith("@northstar.io")) {
				return fail(
					404,
					"not_found",
					"Your organization doesn't use single sign-on. Sign in with your email instead.",
				);
			}
			setSession(northstar); // the identity provider would send them back signed in
			return json(200, { redirect_url: "/" });
		},
	],
	[
		"POST",
		/^\/v1\/auth\/password\/forgot$/,
		(_, b) => (validEmail(b.email) ? accepted() : badEmail()),
	],
	[
		"POST",
		/^\/v1\/auth\/password\/reset$/,
		(_, b) =>
			b.token === "expired"
				? fail(400, "code_expired", "This reset link has expired.")
				: typeof b.password === "string" && b.password.length >= 12
					? json(204)
					: fail(400, "invalid_input", "Use 12 characters or more."),
	],
	[
		"GET",
		/^\/v1\/auth\/invite\/([^/]+)$/,
		(m) =>
			decodeURIComponent(m[1]) === "expired"
				? fail(404, "not_found", "This invitation is no longer valid.")
				: json(200, { org: northstar, inviter: raya, email: ada.email }),
	],
	[
		"POST",
		/^\/v1\/auth\/invite\/([^/]+)\/accept$/,
		(_, b) => (b.code === "000000" ? wrongCode() : signIn()),
	],
	[
		"POST",
		/^\/v1\/auth\/sign-out$/,
		() => {
			setSession(null);
			return json(204);
		},
	],
	[
		"GET",
		/^\/v1\/me$/,
		() => {
			const org = sessionOrg();
			return org ? json(200, me(org)) : signedOut();
		},
	],
	[
		"GET",
		/^\/v1\/org\/members(?:\?.*)?$/,
		(m) => {
			if (!signedIn()) return signedOut();
			const q =
				new URLSearchParams(m[0].split("?")[1] ?? "").get("q")?.toLowerCase() ??
				"";
			return json(200, {
				items: people.filter((p) => p.name.toLowerCase().includes(q)),
				next: null,
			});
		},
	],
	[
		"GET",
		/^\/v1\/org\/teams$/,
		() => (signedIn() ? json(200, { items: teams }) : signedOut()),
	],
	[
		"GET",
		/^\/v1\/approvals(?:\?.*)?$/,
		(m) => {
			if (!signedIn()) return signedOut();
			const state =
				new URLSearchParams(m[0].split("?")[1] ?? "").get("state") ?? "pending";
			const items = approvals.filter(
				(a) => (state === "pending") === (a.state === "pending"),
			);
			return json(200, { items, next: null });
		},
	],
	[
		"POST",
		/^\/v1\/approvals\/([^/]+)\/(approve|deny)$/,
		(m) => {
			if (!signedIn()) return signedOut();
			const approval = approvals.find((a) => a.id === decodeURIComponent(m[1]));
			if (!approval)
				return fail(404, "not_found", "This request no longer exists.");
			if (approval.state !== "pending")
				return fail(409, "conflict", "Someone already decided this.");
			approval.state = m[2] === "approve" ? "approved" : "denied";
			approval.decided_by = ada;
			approval.decided_at = new Date().toISOString();
			resume(approval);
			return json(200, approval);
		},
	],
	[
		"GET",
		/^\/v1\/activity(?:\?.*)?$/,
		() =>
			signedIn() ? json(200, { items: activity, next: null }) : signedOut(),
	],
	[
		"GET",
		/^\/v1\/agents$/,
		() =>
			signedIn()
				? json(200, { items: [yourAgent, supportBot, opsBot, salesBot] })
				: signedOut(),
	],
	[
		"GET",
		/^\/v1\/conversations(?:\?.*)?$/,
		() => {
			if (!signedIn()) return signedOut();
			const items = [...conversations.values()]
				.map((c) => c.conversation)
				.sort((a, b) => b.created_at.localeCompare(a.created_at));
			return json(200, { items, next: null });
		},
	],
	[
		"POST",
		/^\/v1\/conversations$/,
		(_, b) => {
			if (!signedIn()) return signedOut();
			const message = typeof b.message === "string" ? b.message.trim() : "";
			if (!message) return fail(400, "invalid_input", "Write what you need.");
			if (/fail/i.test(message)) {
				// As the core does when the agent can't start: the conversation stays, failed.
				const id = newConversation(message, "failed");
				push(id, {
					kind: "error",
					code: "internal",
					message: "The agent is unavailable.",
				});
				return fail(502, "internal", "The agent is unavailable.");
			}
			const id = newConversation(message, "working");
			turn(id, message);
			const c = conversations.get(id);
			return json(201, c?.conversation);
		},
	],
	[
		"POST",
		/^\/v1\/conversations\/([^/]+)\/messages$/,
		(m, b) => {
			if (!signedIn()) return signedOut();
			const id = decodeURIComponent(m[1]);
			const c = conversations.get(id);
			if (!c || c.conversation.state === "failed")
				return fail(404, "not_found", "Not found.");
			const message = typeof b.message === "string" ? b.message.trim() : "";
			if (!message) return fail(400, "invalid_input", "Write what you need.");
			push(id, { kind: "message", role: "user", text: message });
			turn(id, message);
			return accepted();
		},
	],
	[
		"POST",
		/^\/v1\/conversations\/([^/]+)\/cancel$/,
		(m) => {
			if (!signedIn()) return signedOut();
			const id = decodeURIComponent(m[1]);
			if (!conversations.has(id)) return fail(404, "not_found", "Not found.");
			later([
				() => {
					push(id, {
						kind: "error",
						code: "internal",
						message: "Stopped before it finished.",
					});
					push(id, { kind: "done" });
					setState(id, "idle");
				},
			]);
			return accepted();
		},
	],
	[
		"GET",
		/^\/v1\/grants$/,
		() => {
			if (!signedIn()) return signedOut();
			if (isMember()) return forbidden();
			const now = new Date().toISOString();
			const items = grants.map((g) => ({
				...g,
				state: g.expires_at > now ? "active" : "expired",
			}));
			return json(200, { items });
		},
	],
	[
		"POST",
		/^\/v1\/grants$/,
		(_, b) => {
			if (!signedIn()) return signedOut();
			if (isMember()) return forbidden();
			const p = (b.principal ?? {}) as Record<string, unknown>;
			const expires = typeof b.expires_at === "string" ? b.expires_at : "";
			const now = Date.now();
			const at = Date.parse(expires);
			if (
				!principalExists(p.kind, p.id) ||
				!catalog.includes(String(b.tool)) ||
				!(at > now && at <= now + 90 * 86_400_000)
			)
				return fail(400, "invalid_input", "Check the grant and try again.");
			const same = (g: Grant) =>
				g.principal.kind === p.kind &&
				g.principal.id === p.id &&
				g.tool === b.tool;
			const live = grants.find(
				(g) => same(g) && Date.parse(g.expires_at) > now,
			);
			if (live) return fail(409, "conflict", "They already have this tool.");
			// An expired grant for the same principal and tool gives way, as in the core.
			for (const g of grants.filter(same)) grants.splice(grants.indexOf(g), 1);
			const grant: Grant = {
				id: `grt_${Math.random().toString(36).slice(2, 8)}`,
				principal: {
					kind: p.kind as Grant["principal"]["kind"],
					id: String(p.id),
				},
				tool: String(b.tool),
				standing: b.standing === true,
				expires_at: expires,
				state: "active",
				granted_by: ada,
				reason: typeof b.reason === "string" && b.reason ? b.reason : null,
			};
			grants.push(grant);
			return json(201, grant);
		},
	],
	[
		"DELETE",
		/^\/v1\/grants\/([^/]+)$/,
		(m) => {
			if (!signedIn()) return signedOut();
			if (isMember()) return forbidden();
			const i = grants.findIndex((g) => g.id === decodeURIComponent(m[1]));
			if (i < 0) return fail(404, "not_found", "Not found.");
			grants.splice(i, 1);
			return json(204);
		},
	],
];

/** Answers one call the way the contract says the core will. */
export async function standIn(
	method: Method,
	path: string,
	body?: unknown,
): Promise<Response> {
	await new Promise((r) => setTimeout(r, 250)); // feel like a network round trip
	for (const [m, pattern, handle] of routes) {
		const match = path.match(pattern);
		if (m === method && match) return handle(match, (body ?? {}) as Body);
	}
	return fail(404, "not_found", "Not found.");
}
