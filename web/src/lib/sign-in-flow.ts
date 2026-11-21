import type { ApiError, Org, SignInOutcome } from "#/lib/api/index.ts";

/**
 * What a multi-step sign-in carries from one screen to the next. Kept in sessionStorage so the
 * pending id stays out of URLs and history; it expires on the core after 10 minutes anyway.
 */
export type Flow = {
	pending?: string;
	email?: string;
	orgs?: Org[];
	methods?: string[];
};

const key = "mandate.sign-in";

export function readFlow(): Flow {
	try {
		return JSON.parse(window.sessionStorage.getItem(key) ?? "{}") as Flow;
	} catch {
		return {};
	}
}

export function saveFlow(flow: Flow) {
	try {
		window.sessionStorage.setItem(key, JSON.stringify(flow));
	} catch {
		// storage off: the next step will find no pending id and send the person back to sign in
	}
}

export function clearFlow() {
	try {
		window.sessionStorage.removeItem(key);
	} catch {
		// nothing to clear
	}
}

export type StepRoute =
	| "/mfa"
	| "/mfa/setup"
	| "/choose-organization"
	| "/verify-email"
	| "/sign-in/required"
	| "/sso";

export type Plan =
	| { kind: "signed_in" }
	| { kind: "step"; to: StepRoute; flow: Flow }
	| { kind: "redirect"; url: string };

/** Where a sign-in outcome (4.2) takes the person next. */
export function planOutcome(outcome: SignInOutcome, email?: string): Plan {
	if (outcome.status === "signed_in") return { kind: "signed_in" };
	const flow: Flow = {
		pending: outcome.pending,
		email,
		orgs: outcome.orgs,
		methods: outcome.methods,
	};
	switch (outcome.next) {
		case "mfa":
			return { kind: "step", to: "/mfa", flow };
		case "mfa_setup":
			return { kind: "step", to: "/mfa/setup", flow };
		case "choose_org":
			return { kind: "step", to: "/choose-organization", flow };
		case "verify_email":
			return { kind: "step", to: "/verify-email", flow };
		case "method_required":
			return { kind: "step", to: "/sign-in/required", flow };
		case "sso_required":
			return outcome.redirect_url
				? { kind: "redirect", url: outcome.redirect_url }
				: { kind: "step", to: "/sso", flow };
	}
}

/** Errors that end the current attempt and take the person elsewhere; the rest show in place. */
export function errorDestination(
	error: ApiError,
): "/no-access" | "/sign-in" | null {
	if (error.code === "no_access") return "/no-access";
	if (error.code === "pending_expired") return "/sign-in";
	return null;
}

/** Messages for the `?error=` the core's SSO callback sends back to /sign-in. */
const callbackMessages: Record<string, string> = {
	no_access: "This account isn't part of an organization on oryxa yet.",
	invalid_credentials:
		"Your identity provider didn't confirm who you are. Try again.",
	pending_expired: "That took too long. Sign in again.",
	org_required: "Choose an organization to sign in to.",
};

export function callbackMessage(code: string | undefined): string | null {
	if (!code) return null;
	return callbackMessages[code] ?? "Signing in didn't finish. Try again.";
}
