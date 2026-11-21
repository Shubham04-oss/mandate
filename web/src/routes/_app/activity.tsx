import { createFileRoute } from "@tanstack/react-router";
import { Page } from "#/components/page.tsx";

export const Route = createFileRoute("/_app/activity")({
	component: () => (
		<Page
			title="Activity"
			hint="The record of what agents did, and under which grant, will appear here."
		/>
	),
});
