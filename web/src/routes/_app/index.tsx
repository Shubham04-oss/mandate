import { createFileRoute, Link } from "@tanstack/react-router";
import {
	type FormEvent,
	type ReactNode,
	type RefObject,
	useRef,
	useState,
} from "react";
import { useStartConversation } from "#/components/agent/use-start-conversation.ts";
import { Icon, type IconName } from "#/components/icons/icon.tsx";
import { StatusMark } from "#/components/icons/status-mark.tsx";
import {
	InputGroup,
	InputGroupAddon,
	InputGroupButton,
	InputGroupInput,
} from "#/components/ui/input-group.tsx";
import {
	Item,
	ItemActions,
	ItemContent,
	ItemDescription,
	ItemMedia,
	ItemTitle,
} from "#/components/ui/item.tsx";
import { Kbd } from "#/components/ui/kbd.tsx";
import { activityRow, handledToday } from "#/lib/activity.ts";
import { api, type Conversation } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { clock, formatWhen, usePref } from "#/lib/preferences.ts";
import { shortcutLabel, useShortcut } from "#/lib/shortcuts.ts";

export const Route = createFileRoute("/_app/")({
	head: () => ({ meta: [{ title: `Home · ${brand.name}` }] }),
	staticData: { title: "Home" },
	ssr: false,
	loader: async () => {
		const [activity, conversations] = await Promise.all([
			api.activity(),
			api.conversations.list(),
		]);
		return { activity: activity.items, conversations: conversations.items };
	},
	component: Home,
});

function Home() {
	const { activity, conversations } = Route.useLoaderData();
	const timeFormat = usePref(clock);
	const handled = handledToday(activity);
	const [ask, setAsk] = useState("");
	const askInput = useRef<HTMLInputElement>(null);
	useShortcut("k", () => askInput.current?.focus());
	return (
		<div className="home-grid">
			<h2
				data-area="greet"
				className="m-0 mb-1 text-center font-semibold text-[24px] tracking-[-0.02em]"
			>
				What do you want to do?
			</h2>
			<AskBox value={ask} onChange={setAsk} inputRef={askInput} />
			<div data-area="below" className="flex flex-col gap-3.5">
				<QuickActions
					onPick={(text) => {
						setAsk(text);
						askInput.current?.focus();
					}}
				/>
				<p className="m-0 text-center text-[12px] text-muted-foreground">
					{handled === 0
						? "Nothing handled yet today."
						: `Agents handled ${handled} ${handled === 1 ? "action" : "actions"} today.`}
				</p>
			</div>
			<WorkingNow conversations={conversations} />
			<Recent
				rows={activity.slice(0, 3).map((a) => activityRow(a, timeFormat))}
			/>
		</div>
	);
}

function AskBox({
	value,
	onChange,
	inputRef,
}: {
	value: string;
	onChange: (value: string) => void;
	inputRef: RefObject<HTMLInputElement | null>;
}) {
	const start = useStartConversation();
	const [sending, setSending] = useState(false);
	const send = async (e: FormEvent) => {
		e.preventDefault();
		const words = value.trim();
		if (!words || sending) return;
		setSending(true);
		// Opens the conversation; on failure the words stay in the box.
		if (await start(words)) onChange("");
		setSending(false);
	};
	return (
		<form data-area="ask" onSubmit={send}>
			<InputGroup className="h-14 rounded-full bg-raised pr-1.5 pl-3.5 shadow-[inset_0_0_0_1px_var(--field),0_8px_24px_rgb(0_0_0/0.2)] has-[[data-slot=input-group-control]:focus-visible]:shadow-[inset_0_0_0_1.5px_var(--text),0_8px_24px_rgb(0_0_0/0.2)]">
				<InputGroupAddon align="inline-start" className="pl-0">
					<span
						aria-hidden="true"
						className="grid size-7 place-items-center text-sea"
					>
						<Icon name="bot" size={20} strokeWidth={1.8} />
					</span>
				</InputGroupAddon>
				<InputGroupInput
					ref={inputRef}
					aria-label="Ask your agent"
					placeholder="Ask your agent to read a page or fill in a form"
					value={value}
					onChange={(e) => onChange(e.target.value)}
					onKeyDown={(e) => {
						if (e.key === "Escape") e.currentTarget.blur();
					}}
					className="text-[14px]"
				/>
				<InputGroupAddon align="inline-end" className="gap-2.5 pr-0">
					<Kbd className="max-sm:hidden">{shortcutLabel("k")}</Kbd>
					<InputGroupButton
						type="submit"
						variant="default"
						size="icon-sm"
						aria-label="Send"
						disabled={!value.trim() || sending}
						className="size-[42px] rounded-full"
					>
						<Icon name="send" size={17} />
					</InputGroupButton>
				</InputGroupAddon>
			</InputGroup>
		</form>
	);
}

// What the v1 tools can do (read a page, submit a form); each fills the box with a start to finish.
const quickActions: {
	title: string;
	meta: string;
	icon: IconName;
	prompt: string;
}[] = [
	{
		title: "Sum up a web page",
		meta: "Reads it as you",
		icon: "link",
		prompt: "Sum up this page: ",
	},
	{
		title: "Fill in a form for me",
		meta: "Asks before it sends",
		icon: "laptop",
		prompt: "Fill in this form for me: ",
	},
	{
		title: "Find something on a site",
		meta: "Reads it as you",
		icon: "search",
		prompt: "Find this on ",
	},
	{
		title: "Compare two pages",
		meta: "Reads both as you",
		icon: "copy",
		prompt: "Compare these two pages: ",
	},
];

function QuickActions({ onPick }: { onPick: (text: string) => void }) {
	return (
		<div className="quick-band rounded-card p-2">
			<ul
				aria-label="Things your agent can do"
				className="m-0 grid list-none grid-cols-2 p-0 gap-2 min-[560px]:grid-cols-4"
			>
				{quickActions.map((action) => (
					<li key={action.title} className="contents">
						<Item
							render={<button type="button" />}
							onClick={() => onPick(action.prompt)}
							className="min-h-24 flex-col flex-nowrap items-start gap-2.5 rounded-[13px] bg-white/10 p-3 text-white shadow-[inset_0_0_0_1px_rgb(255_255_255/0.16)] backdrop-blur-[10px] hover:bg-white/18"
						>
							<ItemMedia className="grid size-[22px] place-items-center text-white">
								<Icon name={action.icon} size={19} strokeWidth={1.8} />
							</ItemMedia>
							<ItemContent className="gap-0.5">
								<ItemTitle className="line-clamp-2 text-[12.5px] text-white leading-[1.35]">
									{action.title}
								</ItemTitle>
								<ItemDescription className="text-[11.5px] text-white/80">
									{action.meta}
								</ItemDescription>
							</ItemContent>
						</Item>
					</li>
				))}
			</ul>
		</div>
	);
}

type Row = {
	id: string;
	kind: string;
	title: string;
	detail: string;
	when: string;
	/** A conversation to open, when the row is one. */
	conversationId?: string;
};

// Bare status marks, nothing behind them: how each row stands, in its own colour.
const marks: Record<string, ReactNode> = {
	working: <StatusMark mark="working" size={18} />,
	waiting: <StatusMark mark="needs-you" size={18} />,
	done: <StatusMark mark="done" size={18} />,
	approved: <StatusMark mark="done" size={18} />,
	refused: <StatusMark mark="failed" size={18} />,
};

/** A side panel: a card with a painted scene fading into it, named by an eyebrow. */
function SidePanel({
	area,
	scene,
	title,
	count,
	more,
	rows,
	empty,
}: {
	area: "left" | "right";
	scene: string;
	title: string;
	count?: number;
	more?: ReactNode;
	rows: Row[];
	/** Said in place of the rows when there are none. */
	empty: string;
}) {
	const id = `${area}-panel`;
	const rowClass =
		"min-h-12 rounded-lg px-2.5 py-1.5 no-underline hover:bg-hover";
	return (
		<section
			data-area={area}
			aria-labelledby={id}
			className="rim flex flex-col gap-0.5 overflow-hidden rounded-window bg-raised px-1.5 pb-1.5"
		>
			<div
				aria-hidden="true"
				className="scene scene-fade-b scene-dim -mx-1.5 h-[76px] bg-[center_60%]"
				style={{ backgroundImage: `url(${scene})` }}
			/>
			<div className="-mt-[30px] relative flex h-9 items-center gap-2 px-2.5">
				<h2 id={id} className="eyebrow">
					{title}
				</h2>
				{count ? (
					<span className="font-semibold text-[12px] text-muted-foreground tabular-nums">
						{count}
					</span>
				) : null}
				<span className="ml-auto text-[12px] text-muted-foreground">
					{more}
				</span>
			</div>
			{rows.length === 0 ? (
				<p className="m-0 px-2.5 pt-1 pb-3 text-[12px] text-muted-foreground leading-normal">
					{empty}
				</p>
			) : null}
			{rows.map((row) => (
				<Item
					key={row.id}
					size="xs"
					render={
						row.conversationId ? (
							<Link
								to="/agent/$conversationId"
								params={{ conversationId: row.conversationId }}
							/>
						) : (
							<button type="button" />
						)
					}
					// rows cut off long titles; the whole line is there on hover
					title={`${row.title}\n${row.detail}`}
					className={rowClass}
				>
					<ItemMedia>{marks[row.kind]}</ItemMedia>
					<ItemContent className="min-w-0 gap-0">
						<ItemTitle className="block w-full truncate font-semibold text-[12.5px]">
							{row.title}
						</ItemTitle>
						<ItemDescription className="truncate text-[11.5px]">
							{row.detail}
						</ItemDescription>
					</ItemContent>
					<ItemActions className="text-[11.5px] text-muted-foreground tabular-nums">
						{row.when}
					</ItemActions>
				</Item>
			))}
		</section>
	);
}

/** Your conversations that are under way or waiting for you, newest first. */
function WorkingNow({ conversations }: { conversations: Conversation[] }) {
	const timeFormat = usePref(clock);
	const rows: Row[] = conversations
		.filter((c) => c.state === "working" || c.state === "waiting")
		.slice(0, 3)
		.map((c) => ({
			id: c.id,
			kind: c.state,
			title: c.title,
			detail: c.state === "waiting" ? "Waiting for you" : "Working",
			when: formatWhen(c.updated_at, timeFormat),
			conversationId: c.id,
		}));
	return (
		<SidePanel
			area="left"
			scene="/scenes/deck.webp"
			title="Working now"
			count={rows.length}
			more={
				<Link
					to="/agent"
					className="text-inherit no-underline hover:text-foreground"
				>
					Open
				</Link>
			}
			rows={rows}
			empty="Nothing in progress. Ask your agent, and it shows up here."
		/>
	);
}

function Recent({ rows }: { rows: Row[] }) {
	return (
		<SidePanel
			area="right"
			scene="/scenes/harbour.webp"
			title="Recent"
			rows={rows}
			empty="Nothing yet. What your agents do shows up here."
		/>
	);
}
