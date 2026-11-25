/**
 * The core's address as a build sees it. A production build must name it: without one the web app
 * would fall back to stand-ins and a deployed app would show made-up people. Called from
 * vite.config.ts, so the build stops before anything is written.
 */
export function checkApiUrl(
	raw: string | undefined,
	build: { command: "build" | "serve"; mode: string },
): string {
	const url = (raw ?? "").trim().replace(/\/+$/, "");
	if (build.command !== "build" || build.mode !== "production") return url;

	const how =
		"Build with VITE_MANDATE_API_URL=https://api.<domain> npm run build (docs/deploy.md, the web app).";
	if (!url) {
		throw new Error(
			`VITE_MANDATE_API_URL is not set. A production build talks to the core, never to stand-ins. ${how}`,
		);
	}
	let parsed: URL;
	try {
		parsed = new URL(url);
	} catch {
		throw new Error(`VITE_MANDATE_API_URL is not a URL: "${url}". ${how}`);
	}
	const local = ["localhost", "127.0.0.1"].includes(parsed.hostname);
	if (parsed.protocol !== "https:" && !(local && parsed.protocol === "http:")) {
		throw new Error(
			`VITE_MANDATE_API_URL must use https (http only for localhost): "${url}".`,
		);
	}
	// The client adds /v1/… itself, so anything after the host would be doubled up.
	if (url !== parsed.origin) {
		throw new Error(
			`VITE_MANDATE_API_URL must be an origin with no path, e.g. https://api.example.com: "${url}".`,
		);
	}
	return url;
}
