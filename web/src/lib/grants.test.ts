import { describe, expect, it } from "vitest";
import { ApiError, type Grant } from "./api/index.ts";
import {
	expiresAt,
	forTool,
	grantChoices,
	grantsAccess,
	principalName,
	searchChoices,
} from "./grants.ts";

describe("the grants admin check", () => {
	it("shows the controls to admins only", () => {
		expect(grantsAccess({ is_admin: true })).toBe("manage");
		expect(grantsAccess({ is_admin: false })).toBe("not_admin");
	});

	it("treats a 403 from the core as not an admin, whatever the page thought", () => {
		expect(
			grantsAccess({ is_admin: true }, new ApiError("forbidden", "No", 403)),
		).toBe("not_admin");
		expect(
			grantsAccess({ is_admin: true }, new ApiError("internal", "Down", 502)),
		).toBe("manage");
	});
});

describe("expiresAt", () => {
	const now = new Date("2026-11-23T10:00:00Z");

	it("adds whole days", () => {
		expect(expiresAt(7, now)).toBe("2026-11-30T10:00:00.000Z");
	});

	it("never goes past the core's 90 days, keeping a minute in hand at the limit", () => {
		expect(expiresAt(90, now)).toBe("2027-02-21T09:59:00.000Z");
		expect(expiresAt(365, now)).toBe(expiresAt(90, now));
		expect(expiresAt(0, now)).toBe(expiresAt(1, now));
	});
});

const grant = (over: Partial<Grant>): Grant => ({
	id: "g",
	principal: { kind: "member", id: "m1" },
	tool: "web.read_page",
	standing: false,
	expires_at: "2026-12-01T00:00:00Z",
	state: "active",
	granted_by: { id: "m0", name: "Ada", email: "ada@x.io" },
	reason: null,
	...over,
});

describe("forTool", () => {
	it("lists one tool's grants, live ones first and soonest to end at the top", () => {
		const list = forTool(
			[
				grant({ id: "late", expires_at: "2027-01-01T00:00:00Z" }),
				grant({
					id: "gone",
					state: "expired",
					expires_at: "2026-11-01T00:00:00Z",
				}),
				grant({ id: "other", tool: "web.submit_form" }),
				grant({ id: "soon", expires_at: "2026-11-25T00:00:00Z" }),
			],
			"web.read_page",
		);
		expect(list.map((g) => g.id)).toEqual(["soon", "late", "gone"]);
	});
});

describe("searching who to allow", () => {
	const choices = grantChoices(
		[
			{ id: "m1", name: "Ada Brennan", email: "ada@northstar.io" },
			{ id: "m2", name: "m2", email: null },
		],
		[{ id: "t1", name: "Platform", members: 3, is_admin: false }],
		[],
	);
	const names = (q: string, picked = "") =>
		searchChoices(choices, q, picked).map((c) => c.name);

	it("includes a member with no email, without failing", () => {
		expect(choices.find((c) => c.principal.id === "m2")?.detail).toBe("");
		expect(names("")).toEqual(["Ada Brennan", "m2", "Platform"]);
		expect(names("m2")).toEqual(["m2"]);
	});

	it("matches names and emails, and keeps the one already picked", () => {
		expect(names("northstar")).toEqual(["Ada Brennan"]);
		expect(names("plat")).toEqual(["Platform"]);
		expect(names("zzz", "member:m2")).toEqual(["m2"]);
	});
});

describe("principalName", () => {
	it("names who a grant is for, or falls back to the id", () => {
		const names = new Map([["team:t1", "Platform"]]);
		expect(principalName({ kind: "team", id: "t1" }, names)).toBe("Platform");
		expect(principalName({ kind: "agent", id: "agt_x" }, names)).toBe("agt_x");
	});
});
