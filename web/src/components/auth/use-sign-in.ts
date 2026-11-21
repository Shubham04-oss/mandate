import { useNavigate, useRouter } from "@tanstack/react-router";
import { useState } from "react";
import { ApiError, type SignInOutcome } from "#/lib/api/index.ts";
import {
	clearFlow,
	errorDestination,
	planOutcome,
	readFlow,
	saveFlow,
} from "#/lib/sign-in-flow.ts";

const unexpected = "Something went wrong. Try again.";

/**
 * Runs sign-in calls for a screen: shows a busy state, follows the outcome to the next step or into
 * the app, and puts errors where the person is looking (or on the screen they belong to).
 */
export function useSignIn() {
	const navigate = useNavigate();
	const router = useRouter();
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const fail = (e: unknown, email?: string) => {
		if (!(e instanceof ApiError)) {
			setError(unexpected);
			return;
		}
		const to = errorDestination(e);
		if (to === "/no-access") {
			saveFlow({ email: email ?? readFlow().email });
			navigate({ to });
		} else if (to === "/sign-in") {
			clearFlow();
			navigate({ to, search: { error: e.code } });
		} else {
			setError(e.message);
		}
	};

	/** A call whose answer is a sign-in outcome. */
	const run = async (
		call: () => Promise<SignInOutcome | undefined>,
		opts: { email?: string; signedInTo?: "/" | "/email-verified" } = {},
	) => {
		setBusy(true);
		setError(null);
		try {
			const outcome = await call();
			if (!outcome) return;
			const plan = planOutcome(outcome, opts.email ?? readFlow().email);
			if (plan.kind === "signed_in") {
				clearFlow();
				await router.invalidate();
				await navigate({ to: opts.signedInTo ?? "/" });
			} else if (plan.kind === "redirect") {
				window.location.assign(plan.url);
			} else {
				saveFlow(plan.flow);
				await navigate({ to: plan.to });
			}
		} catch (e) {
			fail(e, opts.email);
		} finally {
			setBusy(false);
		}
	};

	/** Any other call; `then` runs only when it succeeds. */
	const act = async <T>(
		call: () => Promise<T>,
		then?: (result: T) => void | Promise<void>,
	) => {
		setBusy(true);
		setError(null);
		try {
			const result = await call();
			await then?.(result);
		} catch (e) {
			fail(e);
		} finally {
			setBusy(false);
		}
	};

	return { busy, error, setError, run, act };
}

/** Reads a form field as trimmed text. */
export const field = (data: FormData, name: string) =>
	String(data.get(name) ?? "").trim();
