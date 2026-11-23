import { createFileRoute } from "@tanstack/react-router";
import { ConversationView } from "#/components/agent/conversation-view.tsx";
import { api, type Conversation } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";

export const Route = createFileRoute("/_app/agent/$conversationId")({
	head: () => ({ meta: [{ title: `Your agent · ${brand.name}` }] }),
	staticData: { title: "Your agent" },
	// The session cookie belongs to the API host, so only the browser can load this.
	ssr: false,
	loader: async ({ params }) => {
		const { items } = await api.conversations.list();
		// ponytail: the core has no endpoint for one conversation, so its title and state come from the
		// first page of the list; an older one shows with a plain title. Noted for the lead.
		const conversation: Conversation = items.find(
			(c) => c.id === params.conversationId,
		) ?? {
			id: params.conversationId,
			title: "Conversation",
			state: "idle",
			created_at: "",
			updated_at: "",
		};
		return { conversation, conversations: items };
	},
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
