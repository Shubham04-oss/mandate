import { describe, expect, it } from "vitest";
import { normalizeOtp } from "./otp.ts";

describe("normalizeOtp", () => {
	it("keeps digits only", () => {
		expect(normalizeOtp("123 456")).toBe("123456");
		expect(normalizeOtp("123-456")).toBe("123456");
		expect(normalizeOtp("a1b2c3")).toBe("123");
	});

	it("stops at six digits", () => {
		expect(normalizeOtp("1234567890")).toBe("123456");
	});

	it("returns empty for no digits", () => {
		expect(normalizeOtp("code")).toBe("");
	});
});
