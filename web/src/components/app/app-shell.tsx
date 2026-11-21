import { Link, useMatches } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { Icon, type IconName } from "#/components/icons/icon.tsx";
import { Avatar, AvatarGroup, AvatarImage } from "#/components/ui/avatar.tsx";
import { Button } from "#/components/ui/button.tsx";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "#/components/ui/tooltip.tsx";
import { sample } from "#/lib/sample.ts";

declare module "@tanstack/react-router" {
	interface StaticDataRouteOption {
		/** The page name shown in the header. */
		title?: string;
	}
}

type RailItem = { label: string; icon: IconName; to?: "/"; count?: number };

// ponytail: only Home exists; the rest become links as their pages are built.
const railItems: RailItem[] = [
	{ label: "Home", icon: "home", to: "/" },
	{ label: "Approvals", icon: "shield-check", count: sample.needsYou.length },
	{ label: "Your agent", icon: "bot" },
	{ label: "Teams", icon: "user" },
	{ label: "Activity", icon: "activity" },
];

function RailButton({ item }: { item: RailItem }) {
	const label = item.count
		? `${item.label}, ${item.count} waiting`
		: item.label;
	const content = (
		<>
			<Icon name={item.icon} size={18} strokeWidth={1.8} />
			{item.count ? (
				<span
					aria-hidden="true"
					className="absolute top-[3px] right-[3px] grid h-[15px] min-w-[15px] place-items-center rounded-full bg-foreground px-[3px] font-bold text-[9px] text-canvas"
				>
					{item.count}
				</span>
			) : null}
		</>
	);
	const look =
		"relative size-10 rounded-item text-muted-foreground hover:bg-hover hover:text-foreground data-[status=active]:bg-active data-[status=active]:text-foreground";

	return (
		<Tooltip>
			<TooltipTrigger
				render={
					item.to ? (
						<Button
							variant="ghost"
							size="icon"
							nativeButton={false}
							aria-label={label}
							className={look}
							render={<Link to={item.to} activeOptions={{ exact: true }} />}
						/>
					) : (
						<Button
							variant="ghost"
							size="icon"
							aria-label={label}
							className={look}
						/>
					)
				}
			>
				{content}
			</TooltipTrigger>
			<TooltipContent side="right">{item.label}</TooltipContent>
		</Tooltip>
	);
}

/** "3 need you", hanging from the canvas's top edge on every page. */
function NeedsTab() {
	const waiting = sample.needsYou;
	return (
		// ponytail: opens the approvals page once it exists
		<button
			type="button"
			className="tab-cut group absolute top-0 right-7 z-10 inline-flex h-10 cursor-pointer items-center gap-2 border-0 pr-3.5 pl-2.5 font-semibold text-[12px] text-foreground"
		>
			<AvatarGroup className="-space-x-1.5 *:data-[slot=avatar]:ring-background">
				{waiting.map((item) => (
					<Avatar key={item.id} className="size-5">
						<AvatarImage src={item.avatar} alt="" />
					</Avatar>
				))}
			</AvatarGroup>
			{waiting.length} need you
			<Icon
				name="chevron-right"
				size={14}
				className="text-muted-foreground transition-[transform,color] duration-150 group-hover:translate-x-0.5 group-hover:text-foreground"
			/>
		</button>
	);
}

// The app frame: a collapsed rail and a header on the surround colour, the canvas below them.
// The header is as tall as the rail is wide, so the frame reads evenly on both sides.
export function AppShell({ children }: { children: ReactNode }) {
	const title = useMatches({
		select: (matches) =>
			[...matches].reverse().find((m) => m.staticData.title)?.staticData.title,
	});

	return (
		<TooltipProvider>
			<div className="flex min-h-dvh bg-background text-foreground">
				<nav
					aria-label="Main"
					className="flex w-12 flex-none flex-col items-center gap-1.5 pt-2.5 pb-3"
				>
					<span
						aria-hidden="true"
						className="mb-2.5 grid size-7 place-items-center rounded-sm bg-selected font-bold text-[12px]"
					>
						{sample.org.initial}
					</span>
					{railItems.map((item) => (
						<RailButton key={item.label} item={item} />
					))}
					<span className="flex-1" />
					{/* ponytail: opens the settings pop-up once it exists */}
					<RailButton item={{ label: "Settings", icon: "settings" }} />
					<Avatar className="mt-1.5 size-[30px]">
						<AvatarImage src={sample.me.avatar} alt={sample.me.name} />
					</Avatar>
				</nav>

				<div className="flex min-w-0 flex-1 flex-col">
					<header className="flex h-12 flex-none items-center justify-center bg-background">
						<h1 className="m-0 font-semibold text-[15px]">{title}</h1>
					</header>
					<main className="relative mr-2 flex min-w-0 flex-1 flex-col rounded-t-[17px] bg-canvas">
						<NeedsTab />
						{children}
					</main>
				</div>
			</div>
		</TooltipProvider>
	);
}
