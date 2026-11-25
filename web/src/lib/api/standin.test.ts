import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Me } from "./index.ts";
import { standIn } from "./standin.ts";

// The stand-ins keep their session in the tab's sessionStorage; tests run without a browser.
beforeEach(() => {
	const items = new Map<string, string>();
	vi.stubGlobal("window", {
		sessionStorage: {
			getItem: (k: string) => items.get(k) ?? null,
			setItem: (k: string, v: string) => items.set(k, v),
			removeItem: (k: string) => items.delete(k),
		},
	});
});
afterEach(() => vi.unstubAllGlobals());

const me = async () => {
	const res = await standIn("GET", "/v1/me");
	return { status: res.status, me: res.ok ? ((await res.json()) as Me) : null };
};

describe("stand-in session", () => {
	it("answers /v1/me for the organization chosen at sign-in", async () => {
		await standIn("POST", "/v1/auth/choose-org", {
			pending: "pnd_1",
			org: "org_harbourline",
		});
		expect((await me()).me?.org.name).toBe("Harbourline");
	});

	it("is signed out before sign-in and after sign-out", async () => {
		expect((await me()).status).toBe(401);
		await standIn("POST", "/v1/auth/password", {
			email: "ada@northstar.io",
			password: "anything",
		});
		expect((await me()).me?.org.name).toBe("Northstar");
		await standIn("POST", "/v1/auth/sign-out");
		expect((await me()).status).toBe(401);
	});
});

describe("stand-in sign-up", () => {
	it("needs the organization's name, as the core does", async () => {
		const without = await standIn("POST", "/v1/auth/sign-up", {
			email: "new@northstar.io",
			password: "Harbour1light!",
			name: "Ada",
		});
		expect(without.status).toBe(400);
		expect((await without.json()).error.code).toBe("invalid_input");

		const withOrg = await standIn("POST", "/v1/auth/sign-up", {
			email: "new@northstar.io",
			password: "Harbour1light!",
			name: "Ada",
			org_name: "Northstar",
		});
		expect(await withOrg.json()).toMatchObject({
			status: "next",
			next: "verify_email",
		});
	});
});
