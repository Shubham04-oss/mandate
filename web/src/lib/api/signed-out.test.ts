import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
	ApiError,
	api,
	isLostSession,
	onSignedOut,
	stillSignedIn,
} from "./index.ts";

// The stand-ins answer in tests (no core URL), keeping their session in a stubbed sessionStorage.
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

describe("a lost session", () => {
	it("is a 401 forbidden outside sign-in's own calls", () => {
		const gone = new ApiError("forbidden", "Please sign in.", 401);
		expect(isLostSession(gone, "/v1/me")).toBe(true);
		expect(isLostSession(gone, "/v1/auth/sign-out")).toBe(false);
		expect(
			isLostSession(new ApiError("invalid_credentials", "No", 401), "/v1/me"),
		).toBe(false);
		expect(
			isLostSession(new ApiError("forbidden", "Admins", 403), "/v1/grants"),
		).toBe(false);
	});

	it("goes to sign-in from any call, in one place", async () => {
		const goTo = vi.fn();
		onSignedOut(goTo);
		await expect(api.me()).rejects.toMatchObject({ signedOut: true });
		await expect(api.conversations.list()).rejects.toBeInstanceOf(ApiError);
		expect(goTo).toHaveBeenCalledTimes(2);
	});

	it("is checked once when the event stream gives up: gone means sign in again", async () => {
		const goTo = vi.fn();
		onSignedOut(goTo);
		expect(await stillSignedIn()).toBe(false);
		expect(goTo).toHaveBeenCalledOnce();

		await api.auth.password("ada@northstar.io", "anything");
		expect(await stillSignedIn()).toBe(true);
		expect(goTo).toHaveBeenCalledOnce();
	});

	it("leaves a wrong password to the sign-in page", async () => {
		const goTo = vi.fn();
		onSignedOut(goTo);
		await expect(
			api.auth.password("ada@northstar.io", "wrong"),
		).rejects.toMatchObject({ code: "invalid_credentials", status: 401 });
		expect(goTo).not.toHaveBeenCalled();
	});
});
