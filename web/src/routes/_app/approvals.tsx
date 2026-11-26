import { createFileRoute, getRouteApi } from "@tanstack/react-router";
import { ApprovalList } from "#/components/app/approval-list.tsx";
import { brand } from "#/lib/brand.ts";

const appRoute = getRouteApi("/_app");

export const Route = createFileRoute("/_app/approvals")({
	head: () => ({ meta: [{ title: `Approvals · ${brand.name}` }] }),
	staticData: { title: "Approvals" },
	component: Approvals,
});

// The app frame already loads what's waiting (and refreshes it), so the page and the count agree.
// ponytail: the first page only (50, newest first); add "Show more" with `next` if inboxes grow.
function Approvals() {
	return <ApprovalList approvals={appRoute.useLoaderData().pending} />;
}
