import { createFileRoute, Link } from "@tanstack/react-router";
import { Composer } from "#/components/agent/composer.tsx";
import { StateMark } from "#/components/agent/marks.tsx";
import { useStartConversation } from "#/components/agent/use-start-conversation.ts";
import { keepOnFailedRefresh } from "#/components/app/keep-on-refresh.ts";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { clock, formatWhen, usePref } from "#/lib/preferences.ts";

export const Route = createFileRoute("/_app/agent/")({
	head: () => ({ meta: [{ title: `Your agent · ${brand.name}` }] }),
	staticData: { title: "Your agent" },
	// The session cookie belongs to the API host, so only the browser can load this.
	ssr: false,
	loader: keepOnFailedRefresh(async () => ({
		conversations: (await api.conversations.list()).items,
	})),
	component: NewConversation,
});

/** A new conversation: what should your agent do, and the ones before it. */
function NewConversation() {
	const { conversations } = Route.useLoaderData();
	const start = useStartConversation();
	const timeFormat = usePref(clock);
	return (
		<div className="flex flex-1 flex-col items-center px-4 pt-[12vh] pb-10">
			<div className="flex w-full max-w-[620px] flex-col gap-5">
				<h2 className="m-0 text-center font-semibold text-[24px] tracking-[-0.02em]">
					What should your agent do?
				</h2>
				<Composer
					label="Ask your agent"
					placeholder="Ask your agent to read a page or fill in a form"
					onSend={start}
					autoFocus
				/>
				{conversations.length ? (
					<section
						aria-labelledby="earlier-h"
						className="mt-8 flex flex-col gap-1"
					>
						<h3 id="earlier-h" className="eyebrow mb-2">
							Your conversations
						</h3>
						{conversations.map((c) => (
							<Link
								key={c.id}
								to="/agent/$conversationId"
								params={{ conversationId: c.id }}
								className="flex h-10 items-center gap-3 rounded-[10px] px-2.5 text-[13.5px] text-soft-foreground no-underline hover:bg-hover hover:text-foreground"
							>
								<span className="flex-1 truncate">{c.title}</span>
								<StateMark state={c.state} />
								<span className="text-[12px] text-muted-foreground tabular-nums">
									{formatWhen(c.updated_at, timeFormat)}
								</span>
							</Link>
						))}
					</section>
				) : null}
			</div>
		</div>
	);
}
