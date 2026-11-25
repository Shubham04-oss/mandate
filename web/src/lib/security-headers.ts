/** A fresh nonce for one response's inline scripts: 128 random bits, base64. */
export function newNonce(): string {
	const bytes = crypto.getRandomValues(new Uint8Array(16));
	return btoa(String.fromCharCode(...bytes));
}

/** The core's origin for connect-src, or nothing when the URL is missing or malformed. */
function apiOrigin(apiUrl: string): string | null {
	try {
		return apiUrl ? new URL(apiUrl).origin : null;
	} catch {
		return null;
	}
}

/** The page's Content-Security-Policy: scripts only from here or with this response's nonce. */
export function contentSecurityPolicy(nonce: string, apiUrl: string): string {
	const api = apiOrigin(apiUrl);
	return [
		"default-src 'self'",
		`script-src 'self' 'nonce-${nonce}'`,
		"style-src 'self' 'unsafe-inline'",
		"img-src 'self' data: https:",
		"font-src 'self'",
		`connect-src 'self'${api ? ` ${api}` : ""}`,
		"frame-ancestors 'none'",
		"base-uri 'none'",
		"form-action 'self'",
		"object-src 'none'",
	].join("; ");
}

export function securityHeaders(
	nonce: string,
	apiUrl: string,
): Record<string, string> {
	return {
		"Content-Security-Policy": contentSecurityPolicy(nonce, apiUrl),
		"X-Content-Type-Options": "nosniff",
		"Referrer-Policy": "strict-origin-when-cross-origin",
		"Strict-Transport-Security": "max-age=31536000",
	};
}

/** The response with the security headers set, copied first since a response's headers may be read-only. */
export function withSecurityHeaders(
	response: Response,
	nonce: string,
	apiUrl: string,
): Response {
	const secured = new Response(response.body, response);
	for (const [name, value] of Object.entries(securityHeaders(nonce, apiUrl)))
		secured.headers.set(name, value);
	return secured;
}
