// Hints shown while typing. WorkOS enforces the real password policy; these only guide.
export const passwordRules = [
	{
		key: "length",
		label: "12 characters or more",
		test: (pw: string) => pw.length >= 12,
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
