/**
 * The one place the web app talks to the core. Shapes and rules follow section 4 of
 * docs/superpowers/specs/2026-11-21-v1-remaining-design.md; where the core differs, the core wins.
 *
 * VITE_MANDATE_API_URL names the core and is fixed at build time. In `npm run dev` and tests, leaving
 * it unset answers every call from contract-shaped stand-ins (./standin.ts). A production build
 * without it fails (./api-url.ts), and the stand-ins are never part of one.
 */

// ── Contract shapes (4.3, 5.2; core/src/api where they differ) ───────────────────────────────────
// Timestamps are RFC 3339 in UTC, as the core writes them.

export type Person = {
	id: string;
	name: string;
	/** Null for a member who left (core/src/api/org.rs `person`). */
	email: string | null;
	avatar_url?: string;
};
export type Org = {
	id: string;
	name: string;
	summary: { people: number; agents: number };
};
export type TeamRef = { id: string; name: string };
export type Team = {
	id: string;
	name: string;
	members: number;
	is_admin: boolean;
};
export type Me = {
	person: Person;
	org: Org;
	orgs: Org[];
	mfa_enabled: boolean;
	is_admin: boolean;
	teams: TeamRef[];
};
/** `kind` and `owner` are null when the agent is gone (core/src/api/agents.rs `agent_view`). */
export type Agent = {
	id: string;
	name: string;
	kind: "personal" | "team" | null;
	owner: Person | TeamRef | null;
	enabled: boolean;
};
export type ApprovalState =
	| "pending"
	| "approved"
	| "denied"
	| "used"
	| "expired";
export type Approval = {
	id: string;
	agent: Agent;
	tool: string;
	args: Record<string, unknown>;
	/** Null for a tool the core no longer serves (core/src/api/approvals.rs). */
	effect: "read" | "write" | "external" | "delete" | null;
	requested_by: Person;
	requested_at: string;
	expires_at: string;
	state: ApprovalState;
	decided_by: Person | null;
	decided_at: string | null;
	can_decide: boolean;
};
export type Activity = {
	id: string;
	at: string;
	agent: Agent;
	member: Person;
	tool: string;
	decision: "run" | "ask" | "refuse";
	reason?: string;
	approval_id?: string;
	approved_by?: Person;
};
export type Page<T> = { items: T[]; next: string | null };

/** A conversation with the person's own agent (core/src/api/conversations.rs). */
export type ConversationState = "working" | "waiting" | "idle" | "failed";
export type Conversation = {
	id: string;
	title: string;
	state: ConversationState;
	created_at: string;
	updated_at: string;
};

/** What a tool call has got to; the core sends no call id or target, only the tool and this. */
export type ToolStatus =
	| "requested"
	| "running"
	| "completed"
	| "denied"
	| "failed";

/** One event of a conversation's stream; `id` is its place in the log, counting from 1. */
export type ConversationEvent = { id: number } & (
	| { kind: "message"; role: "user" | "assistant"; text: string }
	| { kind: "tool"; name: string; status: ToolStatus }
	| { kind: "approval_wait"; approval: Approval }
	| { kind: "approval_done"; approval_id: string; state: ApprovalState }
	| { kind: "error"; code: string; message: string }
	| { kind: "done" }
);

/** Who an admin let use a tool, until when (core/src/api/grants.rs). */
export type GrantPrincipal = { kind: "agent" | "member" | "team"; id: string };
export type Grant = {
	id: string;
	principal: GrantPrincipal;
	tool: string;
	/** Runs without asking each time; reads under a grant run anyway. */
	standing: boolean;
	expires_at: string;
	state: "active" | "expired";
	granted_by: Person;
	reason: string | null;
};
export type NewGrant = {
	principal: GrantPrincipal;
	tool: string;
	standing: boolean;
	/** At most 90 days from now. */
	expires_at: string;
	reason?: string;
};
/** `inviter` is null when no one is named on the invitation (core/src/api/auth.rs). */
export type Invitation = { org: Org; inviter: Person | null; email: string };
export type MfaSetup = { qr_svg: string; secret: string; challenge: string };

// ── Sign-in outcomes (4.2) ───────────────────────────────────────────────────────────────────────

export type NextStep =
	| "mfa"
	| "mfa_setup"
	| "choose_org"
	| "verify_email"
	| "sso_required"
	| "method_required";

export type SignInOutcome =
	| { status: "signed_in"; me: Me }
	| {
			status: "next";
			next: NextStep;
			pending: string;
			orgs?: Org[];
			redirect_url?: string;
			methods?: string[];
	  };

// ── Errors (4.1) ─────────────────────────────────────────────────────────────────────────────────

export type ErrorCode =
	| "invalid_input"
	| "invalid_credentials"
	| "code_invalid"
	| "code_expired"
	| "pending_expired"
	| "mfa_required"
	| "email_unverified"
	| "org_required"
	| "no_access"
	| "rate_limited"
	| "forbidden"
	| "not_found"
	| "conflict"
	| "internal"
	// Not a contract code: the request never reached the core.
	| "unreachable";

const contractCodes = new Set<string>([
	"invalid_input",
	"invalid_credentials",
	"code_invalid",
	"code_expired",
	"pending_expired",
	"mfa_required",
	"email_unverified",
	"org_required",
	"no_access",
	"rate_limited",
	"forbidden",
	"not_found",
	"conflict",
	"internal",
]);

export class ApiError extends Error {
	constructor(
		readonly code: ErrorCode,
		message: string,
		/** HTTP status; 0 when the request never got an answer. */
		readonly status: number,
		/** Seconds to wait, from `Retry-After` on 429. */
		readonly retryAfter?: number,
	) {
		super(message);
		this.name = "ApiError";
	}

	/** No session: the person has to sign in. A wrong password or a two-step demand is a 401 too. */
	get signedOut() {
		return this.status === 401 && this.code === "forbidden";
	}
}

/** Whether an error means the session is gone; sign-in's own calls handle their 401s in place. */
export function isLostSession(error: unknown, path: string): boolean {
	return (
		error instanceof ApiError &&
		error.signedOut &&
		!path.startsWith("/v1/auth/")
	);
}

// A fresh load of the sign-in page, so nothing of the old session stays in memory.
let goToSignIn = () => {
	if (typeof window !== "undefined" && window.location.pathname !== "/sign-in")
		window.location.assign("/sign-in");
};

/** Replaces what happens when the session is gone; for tests. */
export function onSignedOut(fn: () => void) {
	goToSignIn = fn;
}

const fallbackMessage = "Something went wrong. Try again.";

/** Turns any response into the body the contract promises, or throws its error as an ApiError. */
export async function readResponse<T>(res: Response): Promise<T> {
	const text = await res.text();
	if (res.ok) return (text ? JSON.parse(text) : undefined) as T;

	let code: ErrorCode = "internal";
	let message = fallbackMessage;
	try {
		const body = JSON.parse(text) as {
			error?: { code?: unknown; message?: unknown };
		};
		if (
			typeof body.error?.code === "string" &&
			contractCodes.has(body.error.code)
		) {
			code = body.error.code as ErrorCode;
		}
		if (typeof body.error?.message === "string" && body.error.message) {
			message = body.error.message;
		}
	} catch {
		// not JSON: keep the generic message rather than showing raw text
	}
	const retry = Number(res.headers.get("Retry-After"));
	return Promise.reject(
		new ApiError(
			code,
			message,
			res.status,
			Number.isFinite(retry) && retry > 0 ? retry : undefined,
		),
	);
}

// ── Transport ────────────────────────────────────────────────────────────────────────────────────

/** The core's origin, e.g. http://localhost:8080. Empty means stand-ins, in dev and tests only. */
export const apiBaseUrl = (import.meta.env.VITE_MANDATE_API_URL ?? "")
	.trim()
	.replace(/\/+$/, "");

export type Method = "GET" | "POST" | "DELETE";

/** Builds the fetch options every call uses; exported for its test. */
export function requestInit(method: Method, body?: unknown): RequestInit {
	const headers: Record<string, string> = { Accept: "application/json" };
	if (body !== undefined) headers["Content-Type"] = "application/json";
	// CSRF guard the core checks on every state-changing request (4.1).
	if (method !== "GET") headers["X-Mandate"] = "1";
	return {
		method,
		credentials: "include",
		headers,
		body: body === undefined ? undefined : JSON.stringify(body),
	};
}

/** Every call to the core goes through here, so a lost session is handled in this one place. */
async function call<T>(
	method: Method,
	path: string,
	body?: unknown,
): Promise<T> {
	try {
		return await send<T>(method, path, body);
	} catch (e) {
		if (isLostSession(e, path)) goToSignIn();
		throw e;
	}
}

async function send<T>(
	method: Method,
	path: string,
	body?: unknown,
): Promise<T> {
	if (!apiBaseUrl) {
		// Loaded on demand inside the DEV branch, so a production bundle never carries the stand-ins.
		if (import.meta.env.DEV) {
			const { standIn } = await import("#/lib/api/standin.ts");
			return readResponse<T>(await standIn(method, path, body));
		}
		// Never reached in a production build: the build fails without the URL.
		throw new ApiError(
			"unreachable",
			"oryxa isn't set up to reach its server.",
			0,
		);
	}
	let res: Response;
	try {
		res = await fetch(`${apiBaseUrl}${path}`, requestInit(method, body));
	} catch {
		throw new ApiError(
			"unreachable",
			"Can't reach oryxa right now. Check your connection and try again.",
			0,
		);
	}
	return readResponse<T>(res);
}

function query(params: Record<string, string | undefined>): string {
	const q = new URLSearchParams();
	for (const [k, v] of Object.entries(params)) if (v) q.set(k, v);
	const s = q.toString();
	return s ? `?${s}` : "";
}

// ── Endpoints (4.3) ──────────────────────────────────────────────────────────────────────────────

export const api = {
	auth: {
		codeStart: (email: string) =>
			call<void>("POST", "/v1/auth/code/start", { email }),
		codeVerify: (email: string, code: string) =>
			call<SignInOutcome>("POST", "/v1/auth/code/verify", { email, code }),
		password: (email: string, password: string) =>
			call<SignInOutcome>("POST", "/v1/auth/password", { email, password }),
		/** Self-serve sign-up: once the email is verified, the core creates the organization with
		 * this person as its admin. */
		signUp: (input: {
			email: string;
			password: string;
			name: string;
			orgName: string;
		}) =>
			call<SignInOutcome>("POST", "/v1/auth/sign-up", {
				email: input.email,
				password: input.password,
				name: input.name,
				org_name: input.orgName,
			}),
		verifyEmail: (pending: string, code: string) =>
			call<SignInOutcome>("POST", "/v1/auth/verify-email", { pending, code }),
		resendVerification: (pending: string) =>
			call<void>("POST", "/v1/auth/verify-email/resend", { pending }),
		mfaVerify: (pending: string, code: string) =>
			call<SignInOutcome>("POST", "/v1/auth/mfa/verify", { pending, code }),
		/** With `pending` during sign-in; without it when already signed in. */
		mfaSetup: (pending?: string) =>
			call<MfaSetup>("POST", "/v1/auth/mfa/setup", pending ? { pending } : {}),
		/** Answers an outcome during sign-in, nothing (204) when already signed in. */
		mfaEnable: (input: { pending?: string; challenge: string; code: string }) =>
			call<SignInOutcome | undefined>("POST", "/v1/auth/mfa/enable", input),
		chooseOrg: (pending: string, org: string) =>
			call<SignInOutcome>("POST", "/v1/auth/choose-org", { pending, org }),
		/** 404 `not_found` when the email's domain has no SSO. */
		ssoStart: (email: string) =>
			call<{ redirect_url: string }>("POST", "/v1/auth/sso/start", { email }),
		forgotPassword: (email: string) =>
			call<void>("POST", "/v1/auth/password/forgot", { email }),
		resetPassword: (token: string, password: string) =>
			call<void>("POST", "/v1/auth/password/reset", { token, password }),
		invite: (token: string) =>
			call<Invitation>("GET", `/v1/auth/invite/${encodeURIComponent(token)}`),
		acceptInvite: (
			token: string,
			body: { password: string; name: string } | { code: string },
		) =>
			call<SignInOutcome>(
				"POST",
				`/v1/auth/invite/${encodeURIComponent(token)}/accept`,
				body,
			),
		signOut: () => call<void>("POST", "/v1/auth/sign-out"),
	},
	me: () => call<Me>("GET", "/v1/me"),
	org: {
		members: (q?: string, cursor?: string) =>
			call<Page<Person>>("GET", `/v1/org/members${query({ q, cursor })}`),
		teams: () => call<{ items: Team[] }>("GET", "/v1/org/teams"),
	},
	approvals: {
		list: (state: "pending" | "decided", cursor?: string) =>
			call<Page<Approval>>("GET", `/v1/approvals${query({ state, cursor })}`),
		/** 409 `conflict` when someone else decided first. */
		approve: (id: string) =>
			call<Approval>("POST", `/v1/approvals/${encodeURIComponent(id)}/approve`),
		deny: (id: string) =>
			call<Approval>("POST", `/v1/approvals/${encodeURIComponent(id)}/deny`),
	},
	activity: (cursor?: string) =>
		call<Page<Activity>>("GET", `/v1/activity${query({ cursor })}`),
	agents: () => call<{ items: Agent[] }>("GET", "/v1/agents"),
	conversations: {
		/** The person's own conversations, newest first. */
		list: (cursor?: string) =>
			call<Page<Conversation>>("GET", `/v1/conversations${query({ cursor })}`),
		/** 403 `no_access` when the person has no agent yet. */
		start: (message: string) =>
			call<Conversation>("POST", "/v1/conversations", { message }),
		send: (id: string, message: string) =>
			call<void>(
				"POST",
				`/v1/conversations/${encodeURIComponent(id)}/messages`,
				{ message },
			),
		cancel: (id: string) =>
			call<void>("POST", `/v1/conversations/${encodeURIComponent(id)}/cancel`),
	},
	/** Org admins only: anyone else gets 403 `forbidden`. */
	grants: {
		list: () => call<{ items: Grant[] }>("GET", "/v1/grants"),
		/** 409 `conflict` when the principal already has a live grant for the tool. */
		create: (grant: NewGrant) => call<Grant>("POST", "/v1/grants", grant),
		revoke: (id: string) =>
			call<void>("DELETE", `/v1/grants/${encodeURIComponent(id)}`),
	},
};

/** Where a conversation's event stream is, for EventSource; empty with stand-ins. */
export function conversationEventsUrl(id: string): string {
	return apiBaseUrl
		? `${apiBaseUrl}/v1/conversations/${encodeURIComponent(id)}/events`
		: "";
}
