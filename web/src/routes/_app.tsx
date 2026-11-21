import { createFileRoute, Outlet } from "@tanstack/react-router";
import { AppShell } from "#/components/app/app-shell.tsx";

// ponytail: no sign-in check yet; once the core issues sessions, this route sends signed-out
// people to /sign-in.
type AppSearch = { settings?: "tools" };

export const Route = createFileRoute("/_app")({
	validateSearch: (search: Record<string, unknown>): AppSearch =>
		search.settings === "tools" ? { settings: "tools" } : {},
	component: () => (
		<AppShell>
			<Outlet />
		</AppShell>
	),
});
