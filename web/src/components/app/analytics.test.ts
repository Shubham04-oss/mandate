import { createRouter } from "@tanstack/react-router";
import { describe, expect, it } from "vitest";
import { routeTree } from "#/routeTree.gen.ts";
import { identityOf, routePattern } from "./analytics.tsx";

describe("page views by route", () => {
	const router = createRouter({ routeTree });

	it("names a page by its route, never by the values in its address", () => {
		expect(routePattern(router, "/agent/3f2a-conversation")).toBe(
			"/agent/$conversationId",
		);
		expect(routePattern(router, "/agent")).toBe("/agent/");
		expect(routePattern(router, "/")).toBe("/");
		expect(routePattern(router, "/reset-password")).toBe("/reset-password");
		expect(routePattern(router, "/sign-in/code")).toBe("/sign-in/code");
	});

	it("counts an address with no page as not found", () => {
		expect(routePattern(router, "/no-such-page")).toBe("/not-found");
		expect(routePattern(router, "/agent/c-1/SECRET")).toBe("/not-found");
	});
});

describe("who the page is for", () => {
	const me = {
		person: { id: "user_01", name: "Ada", email: "ada@example.com" },
		org: { id: "org_01", name: "Northstar" },
	};

	it("is the core's member and tenant ids, nothing else", () => {
		expect(
			identityOf([
				{ routeId: "__root__" },
				{ routeId: "/_app", loaderData: { me, pending: [] } },
			]),
		).toEqual({ member: "user_01", org: "org_01" });
	});

	it("is no one outside the signed-in app", () => {
		expect(identityOf([{ routeId: "__root__" }, { routeId: "/sign-in" }])).toBe(
			null,
		);
		expect(identityOf([{ routeId: "/_app", loaderData: undefined }])).toBe(
			null,
		);
	});
});
