import { describe, expect, it } from "vitest";
import { checkApiUrl } from "./api-url.ts";

const production = { command: "build", mode: "production" } as const;

describe("the production build guard", () => {
	it("fails a production build that doesn't name the core", () => {
		expect(() => checkApiUrl(undefined, production)).toThrow(
			/VITE_MANDATE_API_URL is not set/,
		);
		expect(() => checkApiUrl("  ", production)).toThrow(/not set/);
	});

	it("takes an https origin, and http only for localhost", () => {
		expect(checkApiUrl("https://api.example.com/", production)).toBe(
			"https://api.example.com",
		);
		expect(checkApiUrl("http://localhost:8080", production)).toBe(
			"http://localhost:8080",
		);
		expect(() => checkApiUrl("http://api.example.com", production)).toThrow(
			/https/,
		);
	});

	it("refuses something that isn't an origin", () => {
		expect(() => checkApiUrl("api.example.com", production)).toThrow(
			/not a URL/,
		);
		expect(() => checkApiUrl("https://api.example.com/v1", production)).toThrow(
			/no path/,
		);
	});

	it("leaves dev and test alone, where stand-ins answer", () => {
		expect(
			checkApiUrl(undefined, { command: "serve", mode: "development" }),
		).toBe("");
		expect(
			checkApiUrl(undefined, { command: "build", mode: "development" }),
		).toBe("");
	});
});
