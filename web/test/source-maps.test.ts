import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// Runs against the production build (npm run build first).
const assets = "dist/client/assets";

describe("the browser build's source maps", () => {
	const files = readdirSync(assets);

	it("are written beside the code, for the deploy to upload to PostHog and then delete", () => {
		expect(files.some((f) => f.endsWith(".js.map"))).toBe(true);
	});

	it("are never pointed at by the code, so a browser never asks for one", () => {
		const pointing = files
			.filter((f) => f.endsWith(".js"))
			.filter((f) => readFileSync(join(assets, f), "utf8").includes("sourceMappingURL"));
		expect(pointing).toEqual([]);
	});
});
