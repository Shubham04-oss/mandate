import { createFileRoute } from "@tanstack/react-router";
import { Page } from "#/components/page.tsx";

export const Route = createFileRoute("/_app/settings")({
	component: () => <Page title="Settings" hint="Settings will appear here." />,
});
