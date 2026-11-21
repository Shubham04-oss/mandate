import { createFileRoute } from "@tanstack/react-router";
import { Page } from "#/components/page.tsx";

export const Route = createFileRoute("/_app/agents")({
	component: () => (
		<Page
			title="Agents"
			hint="Your operator and your teams' bots will appear here."
		/>
	),
});
