import { gunzipSync } from "node:zlib";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

// The real posthog-js, run against a stand-in page whose address, referrer and title all carry a
// secret, to prove what actually leaves the browser.
const page = new URL(
	"https://app.example.com/reset-password?token=SECRET#SECRET",
);
const created: string[] = [];
const sent: { url: string; body: string }[] = [];

async function bodyText(url: string, body: unknown): Promise<string> {
	const bytes =
		body instanceof Blob
			? new Uint8Array(await body.arrayBuffer())
			: typeof body === "string"
				? new TextEncoder().encode(body)
				: new Uint8Array(body as ArrayBuffer);
	// gzip goes as Content-Encoding (no query parameter): spot it by its magic bytes.
	if (bytes[0] === 0x1f && bytes[1] === 0x8b)
		return gunzipSync(bytes).toString();
	const text = new TextDecoder().decode(bytes);
	if (url.includes("compression=base64"))
		return Buffer.from(
			decodeURIComponent(text.replace(/^data=/, "")),
			"base64",
		).toString();
	return text;
}

type Sent = {
	event: string;
	properties: Record<string, unknown>;
	$set_once?: object;
};
const events = () =>
	sent.flatMap(({ body }) => {
		const parsed = JSON.parse(body) as { batch?: Sent[] } | Sent[] | Sent;
		return Array.isArray(parsed)
			? parsed
			: "batch" in parsed && parsed.batch
				? parsed.batch
				: [parsed as Sent];
	});

async function until(check: () => boolean) {
	for (let i = 0; i < 100 && !check(); i++)
		await new Promise((r) => setTimeout(r, 100));
}

beforeAll(() => {
	vi.stubEnv("VITE_POSTHOG_KEY", "phc_test");
	vi.stubGlobal("window", globalThis);
	vi.stubGlobal("location", page);
	vi.stubGlobal("document", {
		location: page,
		referrer: "https://idp.example.com/callback?code=SECRET",
		title: "SECRET conversation",
		cookie: "",
		readyState: "complete",
		visibilityState: "visible",
		addEventListener() {},
		removeEventListener() {},
		createElement(tag: string) {
			created.push(tag);
			return { style: {}, setAttribute() {}, appendChild() {} };
		},
		querySelector: () => null,
		querySelectorAll: () => [],
		getElementsByTagName: () => [],
		body: { appendChild() {} },
		head: { appendChild() {} },
		documentElement: { scrollHeight: 0, clientHeight: 0 },
	});
	vi.stubGlobal("screen", { width: 1440, height: 900 });
	vi.stubGlobal("innerWidth", 1440);
	vi.stubGlobal("innerHeight", 900);
	vi.stubGlobal("history", { pushState() {}, replaceState() {} });
	vi.stubGlobal("addEventListener", () => {});
	vi.stubGlobal("removeEventListener", () => {});
	vi.stubGlobal("fetch", async (url: string, init?: RequestInit) => {
		sent.push({
			url: String(url),
			body: await bodyText(String(url), init?.body),
		});
		return new Response("{}", { status: 200 });
	});
});

afterAll(() => {
	vi.unstubAllGlobals();
	vi.unstubAllEnvs();
});

describe("what posthog-js sends", () => {
	it("never carries the address's query, the referrer, the title or an error message", async () => {
		vi.resetModules();
		const analytics = await import("./analytics.ts");
		analytics.setRoutePatterns((p) =>
			p.startsWith("/agent/") ? "/agent/$conversationId" : p,
		);

		await analytics.pageView(page.pathname, null);
		analytics.track("conversation_stopped");
		page.href = "https://app.example.com/agent/c-123?code=SECRET";
		await analytics.pageView(page.pathname, {
			member: "user_01",
			org: "org_01",
		});
		// What an uncaught error reaches: window.onerror, wrapped by PostHog's exception capture.
		(globalThis as { onerror?: OnErrorEventHandler }).onerror?.(
			"Uncaught Error: SECRET",
			page.href,
			1,
			1,
			new Error("could not parse SECRET"),
		);

		await until(() =>
			["$pageview", "conversation_stopped", "$identify", "$exception"].every(
				(e) => events().some((s) => s.event === e),
			),
		);
		const names = events().map((e) => e.event);
		expect(names).toEqual(
			expect.arrayContaining([
				"$pageview",
				"conversation_stopped",
				"$identify",
				"$groupidentify",
				"$exception",
			]),
		);

		for (const { url, body } of sent) {
			expect(url.startsWith("/ingest/e/"), url).toBe(true);
			expect(body).not.toContain("SECRET");
			expect(body).not.toContain("idp.example.com");
		}
		// No script was added to the page: nothing loads from PostHog's CDN.
		expect(created).not.toContain("script");

		const views = events().filter((e) => e.event === "$pageview");
		expect(views.map((v) => v.properties.$current_url)).toEqual([
			"https://app.example.com/reset-password",
			"https://app.example.com/agent/$conversationId",
		]);
		const identify = events().find((e) => e.event === "$identify");
		expect(identify?.properties.distinct_id).toBe("user_01");
		const group = events().find((e) => e.event === "$groupidentify");
		expect(group?.properties.$group_key).toBe("org_01");
		const error = events().find((e) => e.event === "$exception");
		expect(
			(error?.properties.$exception_list as { type: string }[])[0].type,
		).toBe("Error");
	}, 20_000);
});
