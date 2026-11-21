/**
 * The one place the web app talks to the core. Shapes and rules follow section 4 of
 * docs/superpowers/specs/2026-11-21-v1-remaining-design.md (branch docs/v1-remaining).
 *
 * With VITE_MANDATE_API_URL unset, every call is answered by contract-shaped stand-ins (./standin.ts)
 * so the screens work before the core serves these endpoints. Setting the URL switches to the core;
 * no screen changes.
 */
import { standIn } from "#/lib/api/standin.ts";

// ── Contract shapes (4.3) ────────────────────────────────────────────────────────────────────────
// ponytail: the contract doesn't fix a timestamp format; these assume RFC 3339 strings.

export type Person = {
	id: string;
	name: string;
	email: string;
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
export type Agent = {
	id: string;
	name: string;
	kind: "personal" | "team";
	owner: Person | TeamRef;
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
	effect: string;
	requested_by: Person;
	requested_at: string;
	expires_at: string;
	state: ApprovalState;
	decided_by?: Person;
	decided_at?: string;
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
export type Invitation = { org: Org; inviter: Person; email: string };
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

	/** No session: the person has to sign in. */
	get signedOut() {
		return this.status === 401;
	}
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

/** The core's origin, e.g. http://localhost:8080. Empty means stand-ins. */
export const apiBaseUrl = (import.meta.env.VITE_MANDATE_API_URL ?? "")
	.trim()
	.replace(/\/+$/, "");

export type Method = "GET" | "POST";

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

async function call<T>(
	method: Method,
	path: string,
	body?: unknown,
): Promise<T> {
	if (!apiBaseUrl) return readResponse<T>(await standIn(method, path, body));
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
		signUp: (email: string, password: string, name: string) =>
			call<SignInOutcome>("POST", "/v1/auth/sign-up", {
				email,
				password,
				name,
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
};
