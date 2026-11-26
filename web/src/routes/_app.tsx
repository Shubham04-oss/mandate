import {
	createFileRoute,
	type ErrorComponentProps,
	Outlet,
	useRouter,
} from "@tanstack/react-router";
import { AppShell } from "#/components/app/app-shell.tsx";
import { keepOnFailedRefresh } from "#/components/app/keep-on-refresh.ts";
import {
	isSettingsSection,
	type SettingsSection,
} from "#/components/settings/settings-dialog.tsx";
import { Button } from "#/components/ui/button.tsx";
import { Skeleton } from "#/components/ui/skeleton.tsx";
import { ApiError, api } from "#/lib/api/index.ts";

type AppSearch = { settings?: SettingsSection };

export const Route = createFileRoute("/_app")({
	// The session cookie belongs to the API host, so only the browser can load this data.
	ssr: false,
	validateSearch: (search: Record<string, unknown>): AppSearch =>
		isSettingsSection(search.settings) ? { settings: search.settings } : {},
	// A lost session goes to /sign-in from the API module itself (src/lib/api/index.ts).
	loader: keepOnFailedRefresh(async () => {
		const [me, pending] = await Promise.all([
			api.me(),
			api.approvals.list("pending"),
		]);
		return { me, pending: pending.items };
	}),
	errorComponent: AppUnavailable,
	pendingComponent: AppLoading,
	component: () => (
		<AppShell>
			<Outlet />
		</AppShell>
	),
});

/** The shell's outline while the loader runs, so the page isn't blank. */
function AppLoading() {
	return (
		<output
			aria-label="Loading"
			className="flex min-h-dvh bg-background text-foreground"
		>
			<div className="hidden w-60 shrink-0 flex-col gap-3 border-r p-4 md:flex">
				<Skeleton className="h-7 w-32" />
				<Skeleton className="h-5 w-full" />
				<Skeleton className="h-5 w-4/5" />
				<Skeleton className="h-5 w-3/5" />
			</div>
			<div className="flex flex-1 flex-col gap-4 p-6">
				<Skeleton className="h-7 w-48" />
				<Skeleton className="h-24 w-full max-w-2xl" />
				<Skeleton className="h-24 w-full max-w-2xl" />
			</div>
		</output>
	);
}

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
