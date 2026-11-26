import { createFileRoute, Link, notFound } from "@tanstack/react-router";
import { ConversationView } from "#/components/agent/conversation-view.tsx";
import { ApiError, api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";

export const Route = createFileRoute("/_app/agent/$conversationId")({
	head: () => ({ meta: [{ title: `Your agent · ${brand.name}` }] }),
	staticData: { title: "Your agent" },
	// The session cookie belongs to the API host, so only the browser can load this.
	ssr: false,
	loader: async ({ params }) => {
		try {
			// The conversation itself, and the list for the switcher.
			const [conversation, list] = await Promise.all([
				api.conversations.get(params.conversationId),
				api.conversations.list(),
			]);
			return { conversation, conversations: list.items };
		} catch (e) {
			if (e instanceof ApiError && e.code === "not_found") throw notFound();
			throw e;
		}
	},
	notFoundComponent: NoSuchConversation,
	component: Agent,
});

function Agent() {
	const { conversation, conversations } = Route.useLoaderData();
	// A new view per conversation, so one's events never mix into another's.
	return (
		<ConversationView
			key={conversation.id}
			conversation={conversation}
			conversations={conversations}
		/>
	);
}

function NoSuchConversation() {
	return (
		<div className="grid flex-1 place-items-center p-6 text-center">
			<div className="flex max-w-sm flex-col items-center gap-3">
				<h2 className="m-0 font-semibold text-[17px]">
					There's no conversation here
				</h2>
				<p className="m-0 text-[13px] text-muted-foreground leading-normal">
					It may have been someone else's, or the link is wrong.
				</p>
				<Link to="/agent" className="text-[13px] text-foreground underline">
					Your conversations
				</Link>
			</div>
		</div>
	);
}
