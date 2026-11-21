import { createFileRoute } from "@tanstack/react-router";
import { Page } from "#/components/page.tsx";

export const Route = createFileRoute("/_app/teams")({
	component: () => (
		<Page title="Teams" hint="Teams you belong to will appear here." />
	),
});
