import { TanStackDevtools } from "@tanstack/react-devtools";
import { createRootRoute, HeadContent, Scripts } from "@tanstack/react-router";
import { TanStackRouterDevtoolsPanel } from "@tanstack/react-router-devtools";
import { Toaster } from "#/components/ui/sonner.tsx";

import appCss from "../styles.css?url";

export const Route = createRootRoute({
	head: () => ({
		meta: [
			{
				charSet: "utf-8",
			},
			{
				name: "viewport",
				content: "width=device-width, initial-scale=1",
			},
			{
				title: "oryxa",
			},
		],
		links: [
			{
				rel: "stylesheet",
				href: appCss,
			},
		],
		// Runs before the first paint, so the page never flashes the other theme.
		scripts: [{ children: followDeviceTheme }],
	}),
	shellComponent: RootDocument,
});

// Dark unless the device asks for light; follows the setting when it changes.
// ponytail: no in-app switch yet; the device setting decides.
const followDeviceTheme = `(() => {
	const light = window.matchMedia("(prefers-color-scheme: light)");
	const apply = () => document.documentElement.classList.toggle("light", light.matches);
	apply();
	light.addEventListener("change", apply);
})();`;

function RootDocument({ children }: { children: React.ReactNode }) {
	return (
		// The theme class is set by the script above before React starts.
		<html lang="en" suppressHydrationWarning>
			<head>
				<HeadContent />
			</head>
			<body>
				{children}
				<TanStackDevtools
					config={{
						position: "bottom-right",
					}}
					plugins={[
						{
							name: "Tanstack Router",
							render: <TanStackRouterDevtoolsPanel />,
						},
					]}
				/>
				<Toaster />
				<Scripts />
			</body>
		</html>
	);
}
