import { describe, expect, it } from "vitest";
import {
	contentSecurityPolicy,
	newNonce,
	securityHeaders,
	withSecurityHeaders,
} from "./security-headers.ts";

describe("the security headers", () => {
	it("allow scripts only from the app or with this response's nonce", () => {
		expect(contentSecurityPolicy("abc", "https://api.oryxa.in")).toBe(
			"default-src 'self'; script-src 'self' 'nonce-abc'; style-src 'self' 'unsafe-inline'; " +
				"img-src 'self' data: https:; font-src 'self'; connect-src 'self' https://api.oryxa.in; " +
				"frame-ancestors 'none'; base-uri 'none'; form-action 'self'; object-src 'none'",
		);
	});

	it("take connect-src's origin from the API URL, and leave it out when there is none", () => {
		expect(
			contentSecurityPolicy("n", "https://api.oryxa.in/some/path"),
		).toContain("connect-src 'self' https://api.oryxa.in;");
		expect(contentSecurityPolicy("n", "")).toContain("connect-src 'self';");
		expect(contentSecurityPolicy("n", "not a url")).toContain(
			"connect-src 'self';",
		);
	});

	it("include nosniff, the referrer policy and HSTS", () => {
		expect(securityHeaders("n", "")).toMatchObject({
			"X-Content-Type-Options": "nosniff",
			"Referrer-Policy": "strict-origin-when-cross-origin",
			"Strict-Transport-Security": "max-age=31536000",
		});
	});

	it("are set on a response, keeping its status, body and other headers", async () => {
		const original = new Response("<p>hi</p>", {
			status: 201,
			headers: { "Content-Type": "text/html" },
		});
		const secured = withSecurityHeaders(original, "n", "");
		expect(secured.status).toBe(201);
		expect(secured.headers.get("Content-Type")).toBe("text/html");
		expect(secured.headers.get("X-Content-Type-Options")).toBe("nosniff");
		expect(await secured.text()).toBe("<p>hi</p>");
	});

	it("use a new nonce each time", () => {
		const a = newNonce();
		expect(a).toMatch(/^[A-Za-z0-9+/]{22}==$/);
		expect(newNonce()).not.toBe(a);
	});
});
