import { cloudflare } from "@cloudflare/vite-plugin";
import tailwindcss from "@tailwindcss/vite";
import { devtools } from "@tanstack/devtools-vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { defineConfig, loadEnv } from "vite";
import { checkApiUrl } from "./src/lib/api/api-url.ts";

const config = defineConfig(({ command, mode }) => {
	// A production build must name the core, so a deployed app never answers from stand-ins.
	checkApiUrl(loadEnv(mode, process.cwd(), "VITE_").VITE_MANDATE_API_URL, {
		command,
		mode,
	});
	return {
		resolve: { tsconfigPaths: true },
		plugins: [
			devtools(),
			cloudflare({ viteEnvironment: { name: "ssr" } }),
			tailwindcss(),
			tanstackStart(),
			viteReact(),
		],
	};
});

export default config;
