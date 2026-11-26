import { createFileRoute } from "@tanstack/react-router";
import { ApprovalList } from "#/components/app/approval-list.tsx";
import { PageError, PageLoading } from "#/components/app/page-states.tsx";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";

export const Route = createFileRoute("/_app/approvals")({
	head: () => ({ meta: [{ title: `Approvals · ${brand.name}` }] }),
	staticData: { title: "Approvals" },
	// The session cookie belongs to the API host, so only the browser can load this.
	ssr: false,
	// ponytail: the first page only (50, newest first); add "Show more" with `next` if inboxes grow.
	loader: async () => ({
		approvals: (await api.approvals.list("pending")).items,
	}),
	pendingComponent: PageLoading,
	errorComponent: PageError,
	component: Approvals,
});

function Approvals() {
	return <ApprovalList approvals={Route.useLoaderData().approvals} />;
}
