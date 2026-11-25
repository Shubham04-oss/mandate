import {
	createFileRoute,
	type ErrorComponentProps,
	Outlet,
	useRouter,
} from "@tanstack/react-router";
import { AppShell } from "#/components/app/app-shell.tsx";
import {
	isSettingsSection,
	type SettingsSection,
} from "#/components/settings/settings-dialog.tsx";
import { Button } from "#/components/ui/button.tsx";
import { ApiError, api } from "#/lib/api/index.ts";

type AppSearch = { settings?: SettingsSection };

export const Route = createFileRoute("/_app")({
	// The session cookie belongs to the API host, so only the browser can load this data.
	ssr: false,
	validateSearch: (search: Record<string, unknown>): AppSearch =>
		isSettingsSection(search.settings) ? { settings: search.settings } : {},
	// A lost session goes to /sign-in from the API module itself (src/lib/api/index.ts).
	loader: async () => {
		const [me, pending] = await Promise.all([
			api.me(),
			api.approvals.list("pending"),
		]);
		return { me, pending: pending.items };
	},
	errorComponent: AppUnavailable,
	component: () => (
		<AppShell>
			<Outlet />
		</AppShell>
	),
});

function AppUnavailable({ error }: ErrorComponentProps) {
	const router = useRouter();
	// Already on its way to /sign-in: nothing to say meanwhile.
	if (error instanceof ApiError && error.signedOut) return null;
	return (
		<div className="grid min-h-dvh place-items-center bg-background p-6 text-center text-foreground">
			<div className="flex max-w-sm flex-col items-center gap-3">
				<h1 className="m-0 font-semibold text-[17px]">
					oryxa can't load right now
				</h1>
				<p className="m-0 text-[13px] text-muted-foreground leading-normal">
					{error instanceof ApiError
						? error.message
						: "Something went wrong. Try again."}
				</p>
				<Button onClick={() => router.invalidate()}>Try again</Button>
			</div>
		</div>
	);
}
