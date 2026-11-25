import { defineConfig } from "vitest/config";

// Tests that need the production build (npm run build), run with npm run test:dist.
export default defineConfig({
	test: { include: ["test/**/*.test.ts"], testTimeout: 20_000 },
});
