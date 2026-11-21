import { describe, expect, it } from "vitest";
import {
	searchCatalog,
	summarizeConnections,
	type ToolGroup,
} from "./tools.ts";

describe("summarizeConnections", () => {
	it("counts only what is there", () => {
		expect(
			summarizeConnections([{ status: "connected" }, { status: "connected" }]),
		).toBe("2 connected");
	});

	it("adds connecting and attention when present", () => {
		expect(
			summarizeConnections([
				{ status: "connected" },
				{ status: "connecting" },
				{ status: "expired" },
				{ status: "failed" },
			]),
		).toBe("1 connected · 1 connecting · 2 need attention");
	});

	it("uses the singular for one", () => {
		expect(summarizeConnections([{ status: "expired" }])).toBe(
			"0 connected · 1 needs attention",
		);
	});
});

const tool = (id: string, name: string) => ({
	id,
	name,
	logo: id,
	description: "",
});
const groups: ToolGroup[] = [
	{
		title: "Communication",
		description: "",
		tools: [tool("zoom", "Zoom"), tool("discord", "Discord")],
	},
	{
		title: "Files & knowledge",
		description: "",
		tools: [tool("dropbox", "Dropbox")],
	},
];

describe("searchCatalog", () => {
	it("returns everything for an empty or one-letter query", () => {
		expect(searchCatalog(groups, "")).toBe(groups);
		expect(searchCatalog(groups, " z ")).toBe(groups);
	});

	it("matches app names regardless of case and drops empty groups", () => {
		const result = searchCatalog(groups, "ZOO");
		expect(result).toHaveLength(1);
		expect(result[0].tools.map((t) => t.id)).toEqual(["zoom"]);
	});

	it("keeps a whole group when the query matches its title", () => {
		const result = searchCatalog(groups, "files");
		expect(result).toHaveLength(1);
		expect(result[0].tools.map((t) => t.id)).toEqual(["dropbox"]);
	});

	it("returns nothing when nothing matches", () => {
		expect(searchCatalog(groups, "salesforce")).toEqual([]);
	});
});
