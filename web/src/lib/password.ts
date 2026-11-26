/** The core's floor (`MIN_PASSWORD` in core/src/api/auth.rs); WorkOS's own policy applies on top. */
export const minPassword = 10;
/** The core's cap on names (`Name` in core/src/api/auth.rs) and passwords (`Password`). */
export const maxName = 128;
export const maxPassword = 256;

// Hints shown while typing. WorkOS enforces the real password policy; these only guide.
export const passwordRules = [
	{
		key: "length",
		label: `${minPassword} characters or more`,
		test: (pw: string) => [...pw].length >= minPassword,
	},
	{
		key: "cases",
		label: "Upper and lowercase",
		test: (pw: string) => /\p{Lu}/u.test(pw) && /\p{Ll}/u.test(pw),
	},
	{ key: "number", label: "A number", test: (pw: string) => /\p{N}/u.test(pw) },
	{
		key: "symbol",
		label: "A symbol",
		test: (pw: string) => /[^\p{L}\p{N}\s]/u.test(pw),
	},
] as const;

export type PasswordRule = (typeof passwordRules)[number];

const levels = ["Too weak", "Weak", "Fair", "Good", "Strong"] as const;

export function checkPassword(pw: string) {
	const results = passwordRules.map((rule) => ({ ...rule, ok: rule.test(pw) }));
	const score = results.filter((r) => r.ok).length;
	const missing = results.find((r) => !r.ok);
	const level = levels[score];
	const hint =
		pw.length === 0
			? ""
			: missing
				? `${level}. Add ${missing.label.toLowerCase()} to make it stronger.`
				: `${level}.`;
	return { results, score, level, hint };
}

/**
 * Why sign-up would refuse this password, checked before sending: the core answers a refused password
 * like an existing account (a code that never arrives), so the person must hear it here.
 * ponytail: WorkOS also rejects breached and low-zxcvbn passwords; two of mixed case, numbers and symbols
 * (or 16+ characters) only approximates that, and the verify screen links back here for the rest.
 */
export function passwordProblem(pw: string): string | null {
	const length = [...pw].length;
	if (length < minPassword) return `Use at least ${minPassword} characters.`;
	if (length > maxPassword) return `Use at most ${maxPassword} characters.`;
	const kinds = passwordRules.filter(
		(r) => r.key !== "length" && r.test(pw),
	).length;
	if (kinds < 2 && length < 16)
		return "Mix upper and lowercase letters, numbers and symbols, or make it 16 characters or more.";
	return null;
}

export type SignUpField = "name" | "email" | "org" | "password";

/** What's wrong with each sign-up field, before anything is sent. Empty when all is well. */
export function signUpProblems(
	input: Record<SignUpField, string>,
): Partial<Record<SignUpField, string>> {
	const problems: Partial<Record<SignUpField, string>> = {};
	const name = (v: string, what: string) =>
		!v.trim()
			? `Enter ${what}.`
			: [...v.trim()].length > maxName
				? `Keep it under ${maxName} characters.`
				: null;
	const n = name(input.name, "your name");
	if (n) problems.name = n;
	if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(input.email.trim()))
		problems.email = "Enter an email address like you@company.com.";
	const o = name(input.org, "your organization's name");
	if (o) problems.org = o;
	const p = passwordProblem(input.password);
	if (p) problems.password = p;
	return problems;
}
