import { Link } from "@tanstack/react-router";
import { cn } from "cn";
import { useEffect, useReducer, useRef, useState } from "react";
import { toast } from "sonner";
import { ActivityPanel } from "#/components/agent/activity-panel.tsx";
import { AgentText } from "#/components/agent/agent-text.tsx";
import { Composer } from "#/components/agent/composer.tsx";
import { StateMark } from "#/components/agent/marks.tsx";
import { useStartConversation } from "#/components/agent/use-start-conversation.ts";
import { Icon } from "#/components/icons/icon.tsx";
import { StatusMark } from "#/components/icons/status-mark.tsx";
import {
	Alert,
	AlertDescription,
	AlertTitle,
	alertVariants,
} from "#/components/ui/alert.tsx";
import { Button, buttonVariants } from "#/components/ui/button.tsx";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "#/components/ui/dropdown-menu.tsx";
import { ApiError, api, type Conversation } from "#/lib/api/index.ts";
import {
	applyEvent,
	emptyView,
	type Line,
	WITHHELD,
} from "#/lib/conversation.ts";
import { type Listening, listen } from "#/lib/conversation-stream.ts";

/**
 * A conversation with your agent: the words on the left, what your agent did on the right.
 * Everything shown comes from the core's event stream.
 */
export function ConversationView({
	conversation,
	conversations,
}: {
	conversation: Conversation;
	conversations: Conversation[];
}) {
	const [view, dispatch] = useReducer(applyEvent, emptyView);
	const [lost, setLost] = useState(false);
	const stream = useRef<Listening | null>(null);
	useEffect(() => {
		const listening = listen(conversation.id, dispatch, { onLost: setLost });
		stream.current = listening;
		// A hidden page holds none of the core's streams; it picks up where it was when shown again.
		const onVisibility = () =>
			document.hidden ? listening.pause() : listening.resume();
		document.addEventListener("visibilitychange", onVisibility);
		return () => {
			document.removeEventListener("visibilitychange", onVisibility);
			listening.stop();
			stream.current = null;
		};
	}, [conversation.id]);

	// Failed means the agent never started: the stream can't say so, the list can.
	const failed = conversation.state === "failed";
	const state = failed ? "failed" : (view.state ?? conversation.state);

	const end = useRef<HTMLDivElement>(null);
	const count = view.lines.length;
	useEffect(() => {
		if (count) end.current?.scrollIntoView({ block: "end" });
	}, [count]);

	const start = useStartConversation();
	const firstAsk =
		view.lines.find((l) => l.kind === "you")?.text ?? conversation.title;

	const send = async (text: string) => {
		try {
			await api.conversations.send(conversation.id, text);
			// The stream closed when the last turn ended; this one needs it again.
			stream.current?.resume();
			return true;
		} catch (e) {
			toast(
				e instanceof ApiError ? e.message : "Something went wrong. Try again.",
			);
			return false;
		}
	};
	const stop = async () => {
		try {
			await api.conversations.cancel(conversation.id);
		} catch (e) {
			toast(
				e instanceof ApiError ? e.message : "Something went wrong. Try again.",
			);
		}
	};

	return (
		<div className="flex min-h-0 flex-1 flex-wrap gap-2 p-2">
			<section
				aria-label={conversation.title}
				className="flex min-w-0 flex-[999_1_520px] flex-col px-3 sm:px-7"
			>
				<Switcher current={conversation} conversations={conversations} />
				<div className="flex-1 pt-8 pb-7 sm:pt-10">
					<div className="mx-auto flex max-w-[620px] flex-col gap-[22px]">
						{view.lines.map((line) => (
							<LineView key={line.key} line={line} failed={failed} />
						))}
						{state === "working" ? (
							<p className="m-0 flex items-center gap-2.5 text-[13px] text-muted-foreground">
								<StatusMark mark="working" />
								Your agent is working
							</p>
						) : null}
						{state === "waiting" ? (
							<p className="m-0 flex items-center gap-2.5 text-[13px] text-muted-foreground">
								<StatusMark mark="needs-you" />
								Waiting for your OK
							</p>
						) : null}
						{lost ? (
							<output className={alertVariants()}>
								<StatusMark mark="lost" size={18} />
								<AlertTitle>Lost touch with your agent</AlertTitle>
								<AlertDescription>
									It tries again when you send a message or come back to this
									page.
								</AlertDescription>
							</output>
						) : null}
						<div ref={end} />
					</div>
				</div>
				<div className="sticky bottom-0 bg-linear-to-t from-60% from-canvas to-transparent pt-6 pb-3.5">
					<div className="mx-auto max-w-[620px]">
						{failed ? (
							<div className="rim-lift flex flex-wrap items-center gap-3 rounded-window bg-raised py-3 pr-3 pl-[18px]">
								<span className="flex-1 text-[13px] text-soft-foreground leading-normal">
									This conversation can't take a reply. Start again with the
									same words as a new one.
								</span>
								<Button onClick={() => start(firstAsk)}>Start again</Button>
							</div>
						) : (
							<Composer
								label="Reply to your agent"
								placeholder="Reply to your agent"
								onSend={send}
								working={state === "working"}
								onStop={stop}
							/>
						)}
					</div>
				</div>
			</section>
			<ActivityPanel steps={view.steps} />
		</div>
	);
}

function LineView({ line, failed }: { line: Line; failed: boolean }) {
	if (line.kind === "you")
		return (
			<p className="m-0 max-w-[76%] self-end whitespace-pre-wrap rounded-[18px] bg-hover px-[15px] py-2.5 text-[14px] leading-[1.6]">
				{line.text}
			</p>
		);
	if (line.kind === "error")
		return (
			<Alert variant="destructive">
				<StatusMark mark="error" size={18} />
				<AlertTitle>
					{failed
						? "Your agent couldn't start on this."
						: "Your agent couldn't finish."}
				</AlertTitle>
				<AlertDescription>{line.text}</AlertDescription>
			</Alert>
		);
	if (line.text === WITHHELD)
		return (
			<div className={alertVariants()}>
				<StatusMark mark="info" size={18} className="text-muted-foreground" />
				<AlertDescription>
					A message was held back by the safety check.
				</AlertDescription>
			</div>
		);
	return <AgentText text={line.text} />;
}

/** The conversation's title opens the others; the pencil starts a new one. */
function Switcher({
	current,
	conversations,
}: {
	current: Conversation;
	conversations: Conversation[];
}) {
	return (
		<div className="flex h-11 items-center gap-1">
			<DropdownMenu>
				<DropdownMenuTrigger
					render={
						<Button
							variant="ghost"
							className="h-8 max-w-[min(420px,100%)] gap-1.5 rounded-[9px] px-2.5 font-semibold text-[13.5px] text-foreground hover:bg-hover"
						/>
					}
				>
					<span className="truncate">{current.title}</span>
					<Icon
						name="chevron-right"
						size={13}
						className="rotate-90 text-muted-foreground"
					/>
				</DropdownMenuTrigger>
				<DropdownMenuContent align="start" className="w-80">
					{conversations.map((c) => (
						<DropdownMenuItem
							key={c.id}
							render={
								<Link
									to="/agent/$conversationId"
									params={{ conversationId: c.id }}
								/>
							}
							className={c.id === current.id ? "bg-accent" : undefined}
						>
							<span className="flex-1 truncate">{c.title}</span>
							<StateMark state={c.state} />
						</DropdownMenuItem>
					))}
				</DropdownMenuContent>
			</DropdownMenu>
			{/* A link, not a button: it goes somewhere. */}
			<Link
				to="/agent"
				aria-label="New conversation"
				className={cn(
					buttonVariants({ variant: "ghost", size: "icon" }),
					"size-[30px] rounded-[9px] text-soft-foreground hover:bg-hover",
				)}
			>
				<Icon name="pencil" size={16} strokeWidth={1.9} />
			</Link>
		</div>
	);
}
