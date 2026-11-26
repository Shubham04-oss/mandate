import { defineConfig } from "vitest/config";

// Separate from vite.config.ts so unit tests don't start the Worker runtime.
export default defineConfig({ test: { include: ["src/**/*.test.ts"] } });
