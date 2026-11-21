import { describe, expect, it } from "vitest";
import { checkPassword } from "./password.ts";

describe("checkPassword", () => {
	it("gives no hint for an empty password", () => {
		const result = checkPassword("");
		expect(result.score).toBe(0);
		expect(result.hint).toBe("");
	});

	it("scores each rule once", () => {
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
			"Good. Add 12 characters or more to make it stronger.",
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
