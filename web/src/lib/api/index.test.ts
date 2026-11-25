import { describe, expect, it } from "vitest";
import { ApiError, readResponse, requestInit } from "./index.ts";

describe("requestInit", () => {
	it("always sends credentials and asks for JSON", () => {
		const init = requestInit("GET");
		expect(init.credentials).toBe("include");
		expect(init.headers).toMatchObject({ Accept: "application/json" });
		expect(init.body).toBeUndefined();
	});

	it("marks state-changing requests with X-Mandate and only those", () => {
		expect(requestInit("POST").headers).toMatchObject({ "X-Mandate": "1" });
		expect(requestInit("GET").headers).not.toHaveProperty("X-Mandate");
	});

	it("sends a body as JSON", () => {
		const init = requestInit("POST", { email: "a@b.co" });
		expect(init.headers).toMatchObject({ "Content-Type": "application/json" });
		expect(init.body).toBe('{"email":"a@b.co"}');
	});
});

const res = (status: number, body?: string, headers?: Record<string, string>) =>
	new Response(body ?? null, { status, headers });

describe("readResponse", () => {
	it("returns the JSON body on success and nothing for an empty one", async () => {
		await expect(readResponse(res(200, '{"ok":true}'))).resolves.toEqual({
			ok: true,
		});
		await expect(readResponse(res(204))).resolves.toBeUndefined();
		await expect(readResponse(res(202))).resolves.toBeUndefined();
	});

	it("turns the contract's error body into an ApiError", async () => {
		const err = await readResponse(
			res(
				400,
				'{"error":{"code":"code_invalid","message":"That code isn\'t right."}}',
			),
		).catch((e) => e);
		expect(err).toBeInstanceOf(ApiError);
		expect(err).toMatchObject({
			code: "code_invalid",
			message: "That code isn't right.",
			status: 400,
		});
	});

	it("reads Retry-After on 429", async () => {
		const err = await readResponse(
			res(429, '{"error":{"code":"rate_limited","message":"Slow down."}}', {
				"Retry-After": "30",
			}),
		).catch((e) => e);
		expect(err).toMatchObject({ code: "rate_limited", retryAfter: 30 });
	});

	it("never shows a body that isn't the contract's error", async () => {
		const err = await readResponse(res(502, "<html>Bad gateway</html>")).catch(
			(e) => e,
		);
		expect(err).toMatchObject({
			code: "internal",
			message: "Something went wrong. Try again.",
			status: 502,
		});
	});

	it("treats an unknown code as internal but keeps the message", async () => {
		const err = await readResponse(
			res(400, '{"error":{"code":"brand_new","message":"Nope."}}'),
		).catch((e) => e);
		expect(err).toMatchObject({ code: "internal", message: "Nope." });
	});

	it("flags a missing session", async () => {
		const err = await readResponse(
			res(401, '{"error":{"code":"forbidden","message":"x"}}'),
		).catch((e: ApiError) => e);
		expect((err as ApiError).signedOut).toBe(true);
	});
});
