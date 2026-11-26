import { type ChildProcess, spawn } from "node:child_process";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

// Runs against the production build (npm run build first), served by the Worker runtime.
const port = 4319;
const origin = `http://localhost:${port}`;
let preview: ChildProcess;

beforeAll(async () => {
	// Its own process group, so afterAll stops vite as well as npx and no stale server lingers.
	preview = spawn("npx", ["vite", "preview", "--port", String(port), "--strictPort"], {
		stdio: "ignore",
		detached: true,
	});
	for (let i = 0; i < 60; i++) {
		try {
			await fetch(origin);
			return;
		} catch {
			await new Promise((r) => setTimeout(r, 500));
		}
	}
	throw new Error("vite preview did not start");
}, 40_000);

afterAll(() => {
	if (preview?.pid) process.kill(-preview.pid, "SIGTERM");
});

describe("a server-rendered page", () => {
	it("carries the security headers, and its scripts carry the CSP's nonce", async () => {
		const res = await fetch(`${origin}/sign-in`, {
			headers: { Accept: "text/html" },
		});
		expect(res.status).toBe(200);
		expect(res.headers.get("x-content-type-options")).toBe("nosniff");
		expect(res.headers.get("referrer-policy")).toBe(
			"strict-origin-when-cross-origin",
		);
		expect(res.headers.get("strict-transport-security")).toBe(
			"max-age=31536000",
		);

		const csp = res.headers.get("content-security-policy") ?? "";
		const nonce = /'nonce-([^']+)'/.exec(csp)?.[1];
		expect(nonce).toBeTruthy();
		expect(csp).toContain("frame-ancestors 'none'");

		const html = await res.text();
		const scripts = html.match(/<script\b[^>]*>/g) ?? [];
		expect(scripts.length).toBeGreaterThan(1);
		for (const tag of scripts) expect(tag).toContain(`nonce="${nonce}"`);
	});

	it("leaves static files with their security headers too (public/_headers)", async () => {
		const res = await fetch(`${origin}/favicon.svg`);
		expect(res.status).toBe(200);
		expect(res.headers.get("x-content-type-options")).toBe("nosniff");
		expect(res.headers.get("referrer-policy")).toBe(
			"strict-origin-when-cross-origin",
		);
		expect(res.headers.get("strict-transport-security")).toBe(
			"max-age=31536000",
		);
	});

	it("uses a new nonce for each response", async () => {
		const nonceOf = async () =>
			/'nonce-([^']+)'/.exec(
				(await fetch(`${origin}/sign-in`, { headers: { Accept: "text/html" } }))
					.headers.get("content-security-policy") ?? "",
			)?.[1];
		expect(await nonceOf()).not.toBe(await nonceOf());
	});
});
