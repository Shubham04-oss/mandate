import { createFileRoute } from "@tanstack/react-router";
import { PageError, PageLoading } from "#/components/app/page-states.tsx";
import { RowItem } from "#/components/app/row-item.tsx";
import { activityRow } from "#/lib/activity.ts";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { clock, formatWhen, usePref } from "#/lib/preferences.ts";

export const Route = createFileRoute("/_app/activity")({
	head: () => ({ meta: [{ title: `Activity · ${brand.name}` }] }),
	staticData: { title: "Activity" },
	// The session cookie belongs to the API host, so only the browser can load this.
	ssr: false,
	// ponytail: the newest page only; add "Show more" with `next` when people need further back.
	loader: async () => ({ activity: (await api.activity()).items }),
	pendingComponent: PageLoading,
	errorComponent: PageError,
	component: Activity,
});

/** What agents did, newest first; the core shows each person only what they may see. */
function Activity() {
	const { activity } = Route.useLoaderData();
	const timeFormat = usePref(clock);
	return (
		<div className="flex flex-1 flex-col items-center px-4 pt-10 pb-10">
			<section
				aria-label="What agents did"
				className="rim flex w-full max-w-[620px] flex-col gap-0.5 rounded-window bg-raised p-1.5"
			>
				{activity.length === 0 ? (
					<p className="m-0 px-2.5 py-3 text-[12px] text-muted-foreground">
						Nothing yet. What your agents do shows up here.
					</p>
				) : (
					activity.map((a) => (
						<RowItem
							key={a.id}
							// the page goes back past today, so older rows show their date
							row={{ ...activityRow(a), when: formatWhen(a.at, timeFormat) }}
						/>
					))
				)}
			</section>
		</div>
	);
}
