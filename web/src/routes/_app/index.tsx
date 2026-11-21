import { createFileRoute } from "@tanstack/react-router";
import { cn } from "cn";
import type { FormEvent, ReactNode } from "react";
import {
	DriveMark,
	GmailMark,
	SlackMark,
} from "#/components/icons/brand-marks.tsx";
import { Icon } from "#/components/icons/icon.tsx";
import { Avatar, AvatarImage } from "#/components/ui/avatar.tsx";
import { Badge } from "#/components/ui/badge.tsx";
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
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { sample } from "#/lib/sample.ts";

export const Route = createFileRoute("/_app/")({
	head: () => ({ meta: [{ title: `Home · ${brand.name}` }] }),
	staticData: { title: "Home" },
	ssr: false,
	loader: async () => ({ activity: (await api.activity()).items }),
	component: Home,
});

// ponytail: nothing is sent yet; the ask goes to the core once conversations exist.
const notWired = (e: FormEvent) => e.preventDefault();

function Home() {
	const { activity } = Route.useLoaderData();
	const handled = handledToday(activity);
	return (
		<div className="home-grid">
			<h2
				data-area="greet"
				className="m-0 mb-1 text-center font-semibold text-[24px] tracking-[-0.02em]"
			>
				What do you want to do?
			</h2>
			<AskBox />
			<div data-area="below" className="flex flex-col gap-3.5">
				<QuickActions />
				<p className="m-0 text-center text-[12px] text-muted-foreground">
					{handled === 0
						? "Nothing handled yet today."
						: `Agents handled ${handled} ${handled === 1 ? "action" : "actions"} today.`}{" "}
					{/* ponytail: links to activity once that page exists */}
					<span className="font-semibold text-foreground">See activity</span>
				</p>
			</div>
			<WorkingNow />
			<Recent rows={activity.slice(0, 3).map(activityRow)} />
		</div>
	);
}

function AskBox() {
	return (
		<form data-area="ask" onSubmit={notWired}>
			<InputGroup className="h-14 rounded-full bg-raised pr-1.5 pl-3.5 shadow-[inset_0_0_0_1px_var(--field),0_8px_24px_rgb(0_0_0/0.2)] has-[[data-slot=input-group-control]:focus-visible]:shadow-[inset_0_0_0_1.5px_var(--text),0_8px_24px_rgb(0_0_0/0.2)]">
				<InputGroupAddon align="inline-start" className="pl-0">
					<Avatar className="size-[26px]">
						<AvatarImage src={sample.agent.avatar} alt="" />
					</Avatar>
				</InputGroupAddon>
				<InputGroupInput
					aria-label="Ask your agent"
					placeholder="Ask your agent, or jump to anything"
					className="text-[14px]"
				/>
				<InputGroupAddon align="inline-end" className="gap-2.5 pr-0">
					<Kbd className="max-sm:hidden">⌘K</Kbd>
					<InputGroupButton
						type="submit"
						variant="default"
						size="icon-sm"
						aria-label="Send"
						className="size-[42px] rounded-full"
					>
						<Icon name="send" size={17} />
					</InputGroupButton>
				</InputGroupAddon>
			</InputGroup>
		</form>
	);
}

type Quick = { title: string; meta: string; mark: ReactNode };

const quickActions: Quick[] = [
	{
		title: "Catch me up on #launch",
		meta: "Slack · 42 new",
		mark: <SlackMark />,
	},
	{
		title: "Draft replies to unread mail",
		meta: "Gmail · 6 unread",
		mark: <GmailMark />,
	},
	{ title: "Find the latest board pack", meta: "Drive", mark: <DriveMark /> },
	{
		title: "Do a task on a website",
		meta: "Your agent's browser",
		mark: <Icon name="laptop" size={15} className="text-[#111111]" />,
	},
];

function QuickActions() {
	return (
		<div className="quick-band rounded-2xl p-2">
			<ul
				aria-label="Quick actions from your connected apps"
				className="m-0 grid list-none grid-cols-2 p-0 gap-2 min-[560px]:grid-cols-4"
			>
				{quickActions.map((action) => (
					<li key={action.title} className="contents">
						<Item
							render={<button type="button" />}
							className="min-h-24 flex-col flex-nowrap items-start gap-2.5 rounded-[13px] bg-white/10 p-3 text-white shadow-[inset_0_0_0_1px_rgb(255_255_255/0.16)] backdrop-blur-[10px] hover:bg-white/18"
						>
							<ItemMedia className="grid size-[26px] place-items-center rounded-[7px] bg-white">
								{action.mark}
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
};

const marks: Record<string, ReactNode> = {
	live: (
		<span className="grid size-6 place-items-center rounded-full bg-ok">
			<i className="size-[7px] rounded-full bg-ok-solid" />
		</span>
	),
	agent: (
		<Avatar className="size-6">
			<AvatarImage src={sample.agent.avatar} alt="" />
		</Avatar>
	),
	scheduled: (
		<span className="grid size-6 place-items-center rounded-full bg-selected text-muted-foreground">
			<Icon name="clock" size={14} />
		</span>
	),
	done: (
		<span className="grid size-6 place-items-center rounded-full bg-ok text-ok-foreground">
			<Icon name="check" size={14} strokeWidth={2.4} />
		</span>
	),
	approved: (
		<span className="grid size-6 place-items-center rounded-full bg-ok text-ok-foreground">
			<Icon name="shield-check" size={14} />
		</span>
	),
	refused: (
		<span className="grid size-6 place-items-center rounded-full bg-destructive/15 text-danger-foreground">
			<Icon name="cross-circle" size={14} />
		</span>
	),
	expired: (
		<span className="grid size-6 place-items-center rounded-full bg-selected text-muted-foreground">
			<Icon name="clock" size={14} />
		</span>
	),
};

/** A side panel: a painted scene fading into a surface one step darker than the canvas. */
function SidePanel({
	area,
	scene,
	title,
	count,
	more,
	rows,
}: {
	area: "left" | "right";
	scene: string;
	title: string;
	count?: number;
	more: string;
	rows: Row[];
}) {
	const id = `${area}-panel`;
	return (
		<section
			data-area={area}
			aria-labelledby={id}
			className="flex flex-col gap-0.5 overflow-hidden rounded-2xl bg-background px-1.5 pb-1.5"
		>
			<div
				aria-hidden="true"
				className="scene scene-fade-b scene-dim -mx-1.5 h-[76px] bg-[center_60%]"
				style={{ backgroundImage: `url(${scene})` }}
			/>
			<div className="-mt-[30px] relative flex h-9 items-center gap-2 px-2.5">
				<h2 id={id} className="m-0 font-semibold text-[12.5px]">
					{title}
				</h2>
				{count ? (
					<Badge className="h-[18px] rounded-full bg-active px-1.5 text-[11px] text-foreground">
						{count}
					</Badge>
				) : null}
				{/* ponytail: opens the full list once those pages exist */}
				<span className="ml-auto text-[12px] text-muted-foreground">
					{more}
				</span>
			</div>
			{rows.map((row) => (
				<Item
					key={row.id}
					size="xs"
					render={<button type="button" />}
					className="min-h-12 rounded-lg px-2.5 py-1.5 hover:bg-hover"
				>
					<ItemMedia>{marks[row.kind]}</ItemMedia>
					<ItemContent className="min-w-0 gap-0">
						<ItemTitle className="block w-full truncate text-[12.5px]">
							{row.title}
						</ItemTitle>
						<ItemDescription className="truncate text-[11.5px]">
							{row.detail}
						</ItemDescription>
					</ItemContent>
					<ItemActions
						className={cn("text-[11.5px] text-muted-foreground tabular-nums")}
					>
						{row.when}
					</ItemActions>
				</Item>
			))}
		</section>
	);
}

// ponytail: the contract has no endpoint for work in progress (live browser sessions, scheduled
// runs), so this panel stays on sample data; noted for the lead.
function WorkingNow() {
	return (
		<SidePanel
			area="left"
			scene="/scenes/deck.webp"
			title="Working now"
			count={sample.working.filter((row) => row.kind !== "scheduled").length}
			more="Open"
			rows={sample.working}
		/>
	);
}

function Recent({ rows }: { rows: Row[] }) {
	return (
		<SidePanel
			area="right"
			scene="/scenes/harbour.webp"
			title="Recent"
			more="All"
			rows={rows}
		/>
	);
}
