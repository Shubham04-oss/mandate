import { describe, expect, it, vi } from "vitest";
import { ingest, isIngest, maxBody } from "./ingest.ts";

const key = "phc_test";
const page = "https://app.example.com";

/** A stand-in for PostHog that records what reached it. */
function posthog(res = () => new Response("{}", { status: 200 })) {
	return vi.fn(async (_url: string | URL | Request, _init?: RequestInit) =>
		res(),
	);
}

const post = (path: string, init: RequestInit = {}) =>
	new Request(`${page}${path}`, { method: "POST", body: "{}", ...init });

describe("the /ingest proxy", () => {
	it("is off when the build has no PostHog key", async () => {
		const send = posthog();
		const res = await ingest(post("/ingest/e/"), "", send);
		expect(res.status).toBe(404);
		expect(send).not.toHaveBeenCalled();
	});

	it("knows its own paths", () => {
		expect(isIngest("/ingest")).toBe(true);
		expect(isIngest("/ingest/e/")).toBe(true);
		expect(isIngest("/ingestion")).toBe(false);
		expect(isIngest("/sign-in")).toBe(false);
	});

	it("forwards events to PostHog EU with their query and body", async () => {
		const send = posthog();
		const res = await ingest(
			post("/ingest/e/?ver=1.449.2&compression=gzip-js", {
				body: "payload",
				headers: { "Content-Type": "text/plain" },
			}),
			key,
			send,
		);
		expect(res.status).toBe(200);
		expect(send).toHaveBeenCalledOnce();
		const [url, init] = send.mock.calls[0];
		expect(url).toBe(
			"https://eu.i.posthog.com/e/?ver=1.449.2&compression=gzip-js",
		);
		expect(init?.method).toBe("POST");
		expect(new TextDecoder().decode(init?.body as Uint8Array)).toBe("payload");
		expect(init?.redirect).toBe("manual");
	});

	it("forwards only Content-Type and Content-Encoding: no cookies, auth or client IP", async () => {
		const send = posthog();
		await ingest(
			post("/ingest/e/", {
				headers: {
					"Content-Type": "application/json",
					"Content-Encoding": "gzip",
					Cookie: "mandate_session=secret",
					Authorization: "Bearer secret",
					"X-Forwarded-For": "203.0.113.7",
					"CF-Connecting-IP": "203.0.113.7",
					"X-Real-IP": "203.0.113.7",
					"User-Agent": "Browser/1",
					Referer: "https://app.example.com/reset-password?token=secret",
					Origin: page,
				},
			}),
			key,
			send,
		);
		const headers = new Headers(send.mock.calls[0][1]?.headers);
		expect([...headers.keys()].sort()).toEqual([
			"content-encoding",
			"content-type",
		]);
	});

	it("reaches no other PostHog path and no other host", async () => {
		const send = posthog();
		for (const path of [
			"/ingest",
			"/ingest/",
			"/ingest/e",
			"/ingest/flags/",
			"/ingest/static/array.js",
			"/ingest/array/phc_test/config",
			"/ingest/api/projects/",
			"/ingest/e/x",
			"/ingest//evil.example/e/",
			"/ingest/%2e%2e/e/",
		]) {
			const res = await ingest(post(path), key, send);
			expect(res.status, path).toBe(404);
		}
		expect(send).not.toHaveBeenCalled();
	});

	it("takes only GET and POST", async () => {
		const send = posthog();
		for (const method of ["PUT", "DELETE", "PATCH", "OPTIONS"]) {
			const res = await ingest(
				new Request(`${page}/ingest/e/`, { method }),
				key,
				send,
			);
			expect(res.status, method).toBe(405);
		}
		expect(send).not.toHaveBeenCalled();
		await ingest(new Request(`${page}/ingest/e/?x=1`), key, send);
		expect(send.mock.calls[0][1]?.method).toBe("GET");
		expect(send.mock.calls[0][1]?.body).toBeUndefined();
	});

	it("refuses a body over the cap, declared or streamed", async () => {
		const send = posthog();
		const declared = await ingest(
			post("/ingest/e/", {
				body: "{}",
				headers: { "Content-Length": String(maxBody + 1) },
			}),
			key,
			send,
		);
		expect(declared.status).toBe(413);

		const chunk = new Uint8Array(64 * 1024);
		let sent = 0;
		const stream = new ReadableStream<Uint8Array>({
			pull(c) {
				if (sent > maxBody) c.close();
				else {
					sent += chunk.byteLength;
					c.enqueue(chunk);
				}
			},
		});
		const streamed = await ingest(
			post("/ingest/e/", { body: stream, duplex: "half" } as RequestInit),
			key,
			send,
		);
		expect(streamed.status).toBe(413);
		expect(send).not.toHaveBeenCalled();

		const atCap = await ingest(
			post("/ingest/e/", { body: new Uint8Array(maxBody) }),
			key,
			send,
		);
		expect(atCap.status).toBe(200);
	});

	it("sends back only the status and type: no Set-Cookie, no Location", async () => {
		const send = posthog(
			() =>
				new Response('{"status":1}', {
					status: 200,
					headers: {
						"Content-Type": "application/json",
						"Set-Cookie": "ph=1; Domain=posthog.com",
						"Access-Control-Allow-Origin": "*",
					},
				}),
		);
		const res = await ingest(post("/ingest/e/"), key, send);
		expect(res.status).toBe(200);
		expect([...res.headers.keys()]).toEqual(["content-type"]);
		expect(await res.text()).toBe('{"status":1}');

		const moved = await ingest(
			post("/ingest/e/"),
			key,
			posthog(
				() =>
					new Response(null, {
						status: 302,
						headers: { Location: "https://elsewhere.example/" },
					}),
			),
		);
		expect(moved.status).toBe(302);
		expect(moved.headers.get("location")).toBeNull();
	});

	it("answers 502 when PostHog can't be reached", async () => {
		const send = vi.fn(async () => {
			throw new TypeError("network");
		});
		const res = await ingest(post("/ingest/e/"), key, send);
		expect(res.status).toBe(502);
	});
});
