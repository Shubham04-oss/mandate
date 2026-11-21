import { createFileRoute, Outlet } from "@tanstack/react-router";
import { AppShell } from "#/components/app/app-shell.tsx";

// ponytail: no sign-in check yet; once the core issues sessions, this route sends signed-out
// people to /sign-in.
export const Route = createFileRoute("/_app")({
	component: () => (
		<AppShell>
			<Outlet />
		</AppShell>
	),
});
