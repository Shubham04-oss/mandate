import {
	getRouteApi,
	Link,
	useMatches,
	useNavigate,
	useRouter,
	useSearch,
} from "@tanstack/react-router";
import { type ReactNode, useState } from "react";
import { toast } from "sonner";
import { useLiveRefresh } from "#/components/app/live-refresh.ts";
import { PersonAvatar } from "#/components/auth/parts.tsx";
import { Icon, type IconName } from "#/components/icons/icon.tsx";
import { StatusMark } from "#/components/icons/status-mark.tsx";
import {
	SettingsDialog,
	type SettingsSection,
} from "#/components/settings/settings-dialog.tsx";
import { AvatarGroup } from "#/components/ui/avatar.tsx";
import { Button } from "#/components/ui/button.tsx";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuGroup,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuRadioGroup,
	DropdownMenuRadioItem,
	DropdownMenuSeparator,
	DropdownMenuSub,
	DropdownMenuSubContent,
	DropdownMenuSubTrigger,
	DropdownMenuTrigger,
} from "#/components/ui/dropdown-menu.tsx";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "#/components/ui/tooltip.tsx";
import { describeApproval } from "#/lib/activity.ts";
import { type Approval, api, type Me, type Person } from "#/lib/api/index.ts";
import { shortcutLabel, useShortcut } from "#/lib/shortcuts.ts";
import { signOut } from "#/lib/sign-out.ts";
import {
	chooseTheme,
	parseChoice,
	readChoice,
	themeChoices,
} from "#/lib/theme.ts";

declare module "@tanstack/react-router" {
	interface StaticDataRouteOption {
		/** The page name shown in the header. */
		title?: string;
	}
}

const appRoute = getRouteApi("/_app");

type RailItem = {
	label: string;
	icon: IconName;
	to?: "/" | "/approvals" | "/agent" | "/activity";
	count?: number;
	onClick?: () => void;
	/** Shown in the tooltip, so the shortcut can be found. */
	shortcut?: string;
};

/** A page in the menu; every one is a link. */
type Place = RailItem & { to: NonNullable<RailItem["to"]> };

// ponytail: no Teams page yet; it joins the list when there is one.
const places = (waiting: number): Place[] => [
	{ label: "Home", icon: "home", to: "/" },
	{
		label: "Approvals",
		icon: "shield-check",
		to: "/approvals",
		count: waiting,
	},
	{ label: "Your agent", icon: "bot", to: "/agent" },
	{ label: "Activity", icon: "activity", to: "/activity" },
];

/** Requests the person can decide now. Their own requests are listed too but aren't theirs to decide. */
const decidable = (pending: Approval[]) => pending.filter((a) => a.can_decide);

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
					// orange is the colour of "needs you"; ink on it reads at 7:1
					className="absolute top-[3px] right-[3px] grid h-[15px] min-w-[15px] place-items-center rounded-full bg-gold-deep px-[3px] font-bold text-[#111111] text-[9px]"
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
							render={
								<Link
									to={item.to}
									// Home is only itself; Your agent stays lit inside a conversation.
									activeOptions={{ exact: item.to === "/" }}
								/>
							}
						/>
					) : (
						<Button
							variant="ghost"
							size="icon"
							aria-label={label}
							className={look}
							onClick={item.onClick}
						/>
					)
				}
			>
				{content}
			</TooltipTrigger>
			<TooltipContent side="right">
				{item.label}
				{item.shortcut ? (
					<span className="ml-2 text-muted-foreground">{item.shortcut}</span>
				) : null}
			</TooltipContent>
		</Tooltip>
	);
}

/** "3 need you", hanging from the canvas's top edge on every page. Gone when nothing waits. */
function NeedsTab({ waiting }: { waiting: Approval[] }) {
	if (waiting.length === 0) return null;
	const askers = [
		...new Map(
			waiting.map((a) => [a.requested_by.id, a.requested_by]),
		).values(),
	].slice(0, 3);
	return (
		<Link
			to="/approvals"
			title={waiting.map(describeApproval).join("\n")}
			className="tab-cut group absolute top-0 right-7 z-10 inline-flex h-10 items-center gap-2 border-0 pr-3.5 pl-2.5 font-semibold text-[12px] text-foreground no-underline data-[status=active]:[&>svg:last-child]:hidden"
		>
			<AvatarGroup className="-space-x-1.5 *:data-[slot=avatar]:ring-background">
				{askers.map((person) => (
					<PersonAvatar key={person.id} person={person} className="size-5" />
				))}
			</AvatarGroup>
			<StatusMark mark="needs-you" size={14} />
			{waiting.length} need you
			<Icon
				name="chevron-right"
				size={14}
				className="text-muted-foreground transition-[transform,color] duration-150 group-hover:translate-x-0.5 group-hover:text-foreground"
			/>
		</Link>
	);
}

/** Signs out on the core, then leaves the app; if that fails, says so and stays. */
function useSignOut() {
	const navigate = useNavigate();
	const router = useRouter();
	return () =>
		signOut({
			request: api.auth.signOut,
			leave: async () => {
				await router.invalidate();
				await navigate({ to: "/sign-in" });
			},
			report: (message) => toast(message),
		});
}

function AccountItems({
	person,
	onSettings,
}: {
	person: Person;
	onSettings: () => void;
}) {
	const signOut = useSignOut();
	return (
		<DropdownMenuGroup>
			<DropdownMenuItem onClick={onSettings}>
				<Icon name="settings" strokeWidth={1.8} />
				Settings
			</DropdownMenuItem>
			<ThemeItem />
			<DropdownMenuItem onClick={signOut}>
				<PersonAvatar person={person} className="size-5" />
				<span className="flex-1">{person.name}</span>
				<span className="text-[12px] text-muted-foreground">Sign out</span>
			</DropdownMenuItem>
		</DropdownMenuGroup>
	);
}

/** Light, dark, or whatever the device is set to. Kept on this device only. */
function ThemeItem() {
	const [choice, setChoice] = useState(readChoice);
	const current = themeChoices.find((c) => c.value === choice)?.label;
	return (
		<DropdownMenuSub>
			<DropdownMenuSubTrigger>
				<Icon name="moon" strokeWidth={1.8} />
				<span className="flex-1">Theme</span>
				<span className="text-[12px] text-muted-foreground">{current}</span>
			</DropdownMenuSubTrigger>
			<DropdownMenuSubContent>
				<DropdownMenuRadioGroup
					value={choice}
					onValueChange={(value) => {
						const next = parseChoice(value);
						setChoice(next);
						chooseTheme(next);
					}}
				>
					{themeChoices.map((c) => (
						<DropdownMenuRadioItem key={c.value} value={c.value} closeOnClick>
							{c.label}
						</DropdownMenuRadioItem>
					))}
				</DropdownMenuRadioGroup>
			</DropdownMenuSubContent>
		</DropdownMenuSub>
	);
}

/**
 * Below the split width the rail is gone and the same places open from the header instead, so the
 * canvas keeps the whole width of a phone.
 */
function HeaderMenu({
	me,
	items,
	onSettings,
}: {
	me: Me;
	items: Place[];
	onSettings: () => void;
}) {
	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				render={
					<Button
						variant="ghost"
						aria-label="Menu"
						className="h-8 gap-2 rounded-full pr-2.5 pl-1 text-foreground hover:bg-hover"
					/>
				}
			>
				<span
					aria-hidden="true"
					className="grid size-6 place-items-center rounded-[7px] bg-selected font-bold text-[11px]"
				>
					{me.org.name[0]}
				</span>
				<Icon
					name="menu"
					size={16}
					strokeWidth={1.8}
					className="text-muted-foreground"
				/>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="start" sideOffset={6} className="w-60">
				<DropdownMenuGroup>
					<DropdownMenuLabel>{me.org.name}</DropdownMenuLabel>
					{items.map((item) => (
						<DropdownMenuItem
							key={item.label}
							render={
								<Link
									to={item.to}
									// Home is only itself; Your agent stays lit inside a conversation.
									activeOptions={{ exact: item.to === "/" }}
								/>
							}
							className="data-[status=active]:bg-active data-[status=active]:font-semibold"
						>
							<Icon name={item.icon} strokeWidth={1.8} />
							{item.label}
							{item.count ? (
								<span className="ml-auto grid h-[18px] min-w-[18px] place-items-center rounded-full bg-foreground px-1.5 font-semibold text-[11px] text-canvas">
									{item.count}
								</span>
							) : null}
						</DropdownMenuItem>
					))}
				</DropdownMenuGroup>
				<DropdownMenuSeparator />
				<AccountItems person={me.person} onSettings={onSettings} />
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

/** The person's picture at the foot of the rail, opening their account menu. */
function RailAccount({ me, onSettings }: { me: Me; onSettings: () => void }) {
	return (
		<DropdownMenu>
			<DropdownMenuTrigger
				render={
					<button
						type="button"
						aria-label={`${me.person.name}, account`}
						className="mt-1.5 cursor-pointer rounded-full border-0 bg-transparent p-0"
					/>
				}
			>
				<PersonAvatar person={me.person} className="size-[30px]" />
			</DropdownMenuTrigger>
			<DropdownMenuContent
				side="right"
				align="end"
				sideOffset={8}
				className="w-60"
			>
				<DropdownMenuGroup>
					<DropdownMenuLabel>{me.person.email}</DropdownMenuLabel>
				</DropdownMenuGroup>
				<AccountItems person={me.person} onSettings={onSettings} />
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

// The app frame: a collapsed rail and a header on the surround colour, the canvas below them.
// The header is as tall as the rail is wide, so the frame reads evenly on both sides.
export function AppShell({ children }: { children: ReactNode }) {
	const { me, pending } = appRoute.useLoaderData();
	useLiveRefresh();
	const waiting = decidable(pending);
	const items = places(waiting.length);
	const title = useMatches({
		select: (matches) =>
			[...matches].reverse().find((m) => m.staticData.title)?.staticData.title,
	});
	// Settings is a pop-up kept in the address (?settings=appearance), so it survives a reload and
	// Back closes it.
	const settings = useSearch({ strict: false, select: (s) => s.settings });
	const navigate = useNavigate();
	// Moving between sections replaces the entry, so Back still closes Settings in one step.
	const showSettings = (section: SettingsSection | undefined) =>
		navigate({
			to: ".",
			search: (prev) => ({ ...prev, settings: section }),
			replace: settings !== undefined && section !== undefined,
		});
	const setSettings = (open: boolean) =>
		showSettings(open ? "appearance" : undefined);
	useShortcut(",", () => setSettings(true));

	return (
		<TooltipProvider>
			<div className="flex min-h-dvh bg-background text-foreground">
				<nav
					aria-label="Main"
					className="hidden w-12 flex-none flex-col items-center gap-1.5 pt-2.5 pb-3 split:flex"
				>
					<span
						aria-hidden="true"
						title={me.org.name}
						className="mb-2.5 grid size-7 place-items-center rounded-sm bg-selected font-bold text-[12px]"
					>
						{me.org.name[0]}
					</span>
					{items.map((item) => (
						<RailButton key={item.label} item={item} />
					))}
					<span className="flex-1" />
					<RailButton
						item={{
							label: "Settings",
							icon: "settings",
							onClick: () => setSettings(true),
							shortcut: shortcutLabel(","),
						}}
					/>
					<RailAccount me={me} onSettings={() => setSettings(true)} />
				</nav>

				<div className="flex min-w-0 flex-1 flex-col">
					<header className="relative flex h-12 flex-none items-center justify-center bg-background">
						<div className="absolute top-2 left-2 split:hidden">
							<HeaderMenu
								me={me}
								items={items}
								onSettings={() => setSettings(true)}
							/>
						</div>
						<h1 className="m-0 font-semibold text-[15px]">{title}</h1>
						<PersonAvatar
							person={me.person}
							className="absolute top-2.5 right-3 size-7 split:hidden"
						/>
					</header>
					<main className="relative mx-2 flex min-w-0 flex-1 flex-col rounded-t-[17px] bg-canvas shadow-[inset_0_0_0_1px_var(--canvas-edge)] split:ml-0">
						<NeedsTab waiting={waiting} />
						{children}
					</main>
				</div>
			</div>
			<SettingsDialog section={settings} onSection={showSettings} />
		</TooltipProvider>
	);
}
