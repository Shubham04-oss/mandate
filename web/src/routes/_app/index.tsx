import { createFileRoute } from "@tanstack/react-router";
import { Page } from "#/components/page.tsx";

export const Route = createFileRoute("/_app/")({
	component: () => (
		<Page
			title="Inbox"
			hint="Requests waiting for your approval will appear here."
		/>
	),
});
