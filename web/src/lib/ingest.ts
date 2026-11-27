/**
 * /ingest: PostHog's EU ingestion, served from our own origin so the page's CSP (connect-src 'self')
 * allows it. It forwards only what an event needs and nothing that says who sent it: no cookies, no
 * Authorization, no client IP headers. Off (404) when the build has no PostHog key.
 */

export const ingestPrefix = "/ingest";
const upstream = "https://eu.i.posthog.com";

/**
 * The paths posthog-js uses with our config (src/lib/analytics.ts): events only, since flags,
 * remote config, recordings and external scripts are off. Exact matches, so no other PostHog path
 * (or host) is reachable through here.
 */
const allowedPaths = new Set(["/e/"]);

/** Bigger than any batch of our small events; PostHog's own beacons stop at 64 KiB. */
export const maxBody = 512 * 1024;

// Request headers that travel; everything else (Cookie, Authorization, X-Forwarded-For,
// CF-Connecting-IP, User-Agent, Origin, Referer) stays here.
const forwardedRequestHeaders = ["content-type", "content-encoding"];

export function isIngest(pathname: string): boolean {
	return pathname === ingestPrefix || pathname.startsWith(`${ingestPrefix}/`);
}

const answer = (status: number, headers?: Record<string, string>) =>
	new Response(null, { status, headers });

/** The body, or null once it passes `max` bytes; read in chunks so a huge one is never held. */
async function readCapped(
	body: ReadableStream<Uint8Array>,
	max: number,
): Promise<Uint8Array<ArrayBuffer> | null> {
	const reader = body.getReader();
	const chunks: Uint8Array[] = [];
	let size = 0;
	for (;;) {
		const { done, value } = await reader.read();
		if (done) break;
		size += value.byteLength;
		if (size > max) {
			await reader.cancel();
			return null;
		}
		chunks.push(value);
	}
	const out = new Uint8Array(size);
	let at = 0;
	for (const c of chunks) {
		out.set(c, at);
		at += c.byteLength;
	}
	return out;
}

/**
 * Forwards one /ingest request to PostHog EU, or refuses it.
 * ponytail: the body (often gzipped) isn't read for its project key, so anyone can relay events for
 * any PostHog EU project through here; it reaches nothing but PostHog's event intake.
 */
export async function ingest(
	request: Request,
	key: string,
	send: typeof fetch = fetch,
): Promise<Response> {
	if (!key) return answer(404);
	const url = new URL(request.url);
	const path = url.pathname.slice(ingestPrefix.length);
	if (!allowedPaths.has(path)) return answer(404);
	if (request.method !== "GET" && request.method !== "POST")
		return answer(405, { Allow: "GET, POST" });

	const headers = new Headers();
	for (const name of forwardedRequestHeaders) {
		const value = request.headers.get(name);
		if (value) headers.set(name, value);
	}

	let body: Uint8Array<ArrayBuffer> | undefined;
	if (request.method === "POST" && request.body) {
		const declared = Number(request.headers.get("content-length"));
		if (declared > maxBody) return answer(413);
		const read = await readCapped(request.body, maxBody);
		if (!read) return answer(413);
		body = read;
	}

	let res: Response;
	try {
		res = await send(`${upstream}${path}${url.search}`, {
			method: request.method,
			headers,
			body,
			// A redirect is passed back as is (without its Location), never followed to another host.
			redirect: "manual",
		});
	} catch {
		return answer(502);
	}
	// Only the type travels back: no Set-Cookie, no Location, nothing PostHog sets for its own domain.
	const type = res.headers.get("content-type");
	return new Response(res.body, {
		status: res.status,
		headers: type ? { "Content-Type": type } : {},
	});
}
