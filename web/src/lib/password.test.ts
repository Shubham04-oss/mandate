import { describe, expect, it } from "vitest";
import { checkPassword, passwordProblem, signUpProblems } from "./password.ts";

describe("checkPassword", () => {
	it("gives no hint for an empty password", () => {
		const result = checkPassword("");
		expect(result.score).toBe(0);
		expect(result.hint).toBe("");
	});

	it("scores each rule once", () => {
		expect(checkPassword("abcdefghij").score).toBe(1);
		expect(checkPassword("abcdefghi").score).toBe(0);
		expect(checkPassword("abcdefghijkl").score).toBe(1);
		expect(checkPassword("Abcdefghijkl").score).toBe(2);
		expect(checkPassword("Abcdefghijk1").score).toBe(3);
		expect(checkPassword("Abcdefghij1!").score).toBe(4);
	});

	it("names the first missing rule in the hint", () => {
		expect(checkPassword("Abcdefghijk1").hint).toBe(
			"Good. Add a symbol to make it stronger.",
		);
		expect(checkPassword("Ab1!").hint).toBe(
			"Good. Add 10 characters or more to make it stronger.",
		);
	});

	it("calls a password meeting every rule strong", () => {
		const result = checkPassword("Abcdefghij1!");
		expect(result.level).toBe("Strong");
		expect(result.hint).toBe("Strong.");
	});

	it("does not count spaces as symbols", () => {
		const symbol = checkPassword("abc def").results.find(
			(r) => r.key === "symbol",
		);
		expect(symbol?.ok).toBe(false);
	});

	it("handles letters outside ASCII", () => {
		const cases = checkPassword("Ärger öl").results.find(
			(r) => r.key === "cases",
		);
		expect(cases?.ok).toBe(true);
	});
});

describe("passwordProblem", () => {
	it("holds the core's 10-character floor", () => {
		expect(passwordProblem("Ab1!Ab1!A")).toBe("Use at least 10 characters.");
		expect(passwordProblem("Ab1!Ab1!Ab")).toBeNull();
	});

	it("counts characters, not UTF-16 units, like the core", () => {
		expect(passwordProblem("Ä1!äöüÖÜ😀x")).toBeNull();
		expect(passwordProblem("😀😀😀😀😀Ab1")).toBe(
			"Use at least 10 characters.",
		);
	});

	it("adds no character-kind rule the core doesn't have", () => {
		expect(passwordProblem("qmxzvbtrplwk")).toBeNull();
	});

	it("holds the core's 256-character cap", () => {
		expect(passwordProblem(`Ab1${"x".repeat(254)}`)).toBe(
			"Use at most 256 characters.",
		);
	});
});

describe("signUpProblems", () => {
	const ok = {
		name: "Ada",
		email: "ada@x.io",
		org: "Northstar",
		password: "Abcdefghij1!",
	};

	it("finds nothing wrong with a good form", () => {
		expect(signUpProblems(ok)).toEqual({});
	});

	it("names each wrong field", () => {
		const p = signUpProblems({
			name: " ",
			email: "ada@",
			org: "",
			password: "short",
		});
		expect(Object.keys(p).sort()).toEqual(["email", "name", "org", "password"]);
	});

	it("holds the core's name cap", () => {
		expect(signUpProblems({ ...ok, org: "x".repeat(129) }).org).toMatch(/128/);
	});
});
