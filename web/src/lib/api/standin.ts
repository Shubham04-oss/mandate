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
 * verify@, sso@, method@ and noaccess@; any other email signs straight in. The code 000000 is always
 * wrong, the password "wrong" never matches, and the reset or invite token "expired" has expired.
 */
import type {
	Activity,
	Agent,
	Approval,
	Me,
	Method,
	Org,
	Person,
	SignInOutcome,
	Team,
} from "#/lib/api/index.ts";

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
	is_admin: true,
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
const signedIn = () => store()?.getItem(sessionKey) === "1";
const setSignedIn = (on: boolean) =>
	on ? store()?.setItem(sessionKey, "1") : store()?.removeItem(sessionKey);

const signIn = (org?: Org): Response => {
	setSignedIn(true);
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
			return next("method_required", { methods: ["sso", "magic_code"] });
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
const badEmail = () => fail(400, "invalid_input", "Enter a valid email.");
const wrongCode = () =>
	fail(400, "code_invalid", "That code isn't right. Check it and try again.");
const accepted = () => json(202);
const signedOut = () => fail(401, "forbidden", "Sign in to continue.");

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
		(_, b) => (validEmail(b.email) ? next("verify_email") : badEmail()),
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
			setSignedIn(true); // the identity provider would send them back signed in
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
			setSignedIn(false);
			return json(204);
		},
	],
	["GET", /^\/v1\/me$/, () => (signedIn() ? json(200, me()) : signedOut())],
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
