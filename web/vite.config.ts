import { resolve } from "node:path";
import { cloudflare } from "@cloudflare/vite-plugin";
import tailwindcss from "@tailwindcss/vite";
import { devtools } from "@tanstack/devtools-vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { defineConfig, loadEnv, type Plugin } from "vite";
import { checkApiUrl } from "./src/lib/api/api-url.ts";

// Design previews with made-up people; dev only.
const previewRoutes = [resolve("src/routes/preview.popups.tsx")];

/**
 * A production build gets a stub for each preview route: the route tree stays as committed, the
 * page is not found, and the preview, its sample people and their pictures never reach the bundle.
 */
function stubPreviewsInBuild(): Plugin {
	return {
		name: "oryxa:stub-previews",
		apply: "build",
		enforce: "pre",
		load(id) {
			if (!previewRoutes.includes(id)) return;
			return [
				'import { createFileRoute, notFound } from "@tanstack/react-router";',
				'export const Route = createFileRoute("/preview/popups")({',
				"\tbeforeLoad: () => {",
				"\t\tthrow notFound();",
				"\t},",
				"});",
			].join("\n");
		},
	};
}

const config = defineConfig(({ command, mode }) => {
	// A production build must name the core, so a deployed app never answers from stand-ins.
	checkApiUrl(loadEnv(mode, process.cwd(), "VITE_").VITE_MANDATE_API_URL, {
		command,
		mode,
	});
	return {
		resolve: { tsconfigPaths: true },
		// Fonts stay files: the CSP's font-src is 'self', which refuses data: URLs.
		build: {
			assetsInlineLimit: (file: string) =>
				/\.(woff2?|ttf|otf)$/.test(file) ? false : undefined,
		},
		// Browser maps without a sourceMappingURL: the deploy uploads them to PostHog for error
		// tracking and deletes them before publishing, so the source is never served.
		environments: { client: { build: { sourcemap: "hidden" } } },
		plugins: [
			stubPreviewsInBuild(),
			devtools(),
			cloudflare({ viteEnvironment: { name: "ssr" } }),
			tailwindcss(),
			tanstackStart(),
			viteReact(),
		],
	};
});

export default config;
