import { getRouteApi } from "@tanstack/react-router";
import { cn } from "cn";
import { type ReactNode, useRef } from "react";
import { Icon, type IconName } from "#/components/icons/icon.tsx";
import { AppearanceSection } from "#/components/settings/appearance-section.tsx";
import { NotificationsSection } from "#/components/settings/notifications-section.tsx";
import { ToolsSection } from "#/components/settings/tools-section.tsx";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogTitle,
} from "#/components/ui/dialog.tsx";

const appRoute = getRouteApi("/_app");

/** The sections that are built, each kept in the address as ?settings=<section>. */
const pages = {
	appearance: AppearanceSection,
	notifications: NotificationsSection,
	tools: ToolsSection,
} satisfies Record<string, () => ReactNode>;

export type SettingsSection = keyof typeof pages;

export const isSettingsSection = (v: unknown): v is SettingsSection =>
	typeof v === "string" && Object.hasOwn(pages, v);

type Section = {
	id:
		| SettingsSection
		| "account"
		| "agent"
		| "organization"
		| "people"
		| "teams";
	label: string;
	icon: IconName;
};

// ponytail: Account, Your agent and the organization's sections need the core; they show the
// shape settings will take until they are built.
const groups = (
	orgName: string,
): { id: string; title: string; sections: Section[] }[] => [
	{
		id: "you",
		title: "You",
		sections: [
			{ id: "account", label: "Account", icon: "user" },
			{ id: "agent", label: "Your agent", icon: "bot" },
			{ id: "appearance", label: "Appearance", icon: "moon" },
			{ id: "notifications", label: "Notifications", icon: "mail" },
		],
	},
	{
		id: "org",
		title: orgName,
		sections: [
			{ id: "organization", label: "Organization", icon: "building" },
			{ id: "people", label: "People", icon: "user" },
			{ id: "teams", label: "Teams", icon: "user" },
		],
	},
	{
		id: "connections",
		title: "Connections",
		sections: [{ id: "tools", label: "Tools", icon: "link" }],
	},
];

/** Settings open over the app rather than as a page, so the person keeps their place. */
export function SettingsDialog({
	section,
	onSection,
}: {
	/** The open section; none means closed. */
	section: SettingsSection | undefined;
	onSection: (section: SettingsSection | undefined) => void;
}) {
	const { me } = appRoute.useLoaderData();
	// the last page stays while the dialog fades out, so it doesn't empty mid-animation
	const shown = useRef(section);
	if (section) shown.current = section;
	const Page = shown.current ? pages[shown.current] : null;
	return (
		<Dialog
			open={section !== undefined}
			onOpenChange={(open) => {
				if (!open) onSection(undefined);
			}}
		>
			<DialogContent className="h-[min(760px,calc(100dvh-2rem))] w-[min(1080px,calc(100%-2rem))] max-w-none rounded-[28px] max-[720px]:h-dvh max-[720px]:w-full max-[720px]:max-w-full max-[720px]:rounded-none max-[720px]:border-0 min-[721px]:flex-row">
				<nav
					aria-label="Settings"
					className="flex flex-none flex-col gap-4 bg-background px-3 pt-5 pb-3 max-[720px]:flex-row max-[720px]:items-center max-[720px]:gap-1 max-[720px]:overflow-x-auto max-[720px]:pt-3 max-[720px]:pr-14 min-[721px]:w-56"
				>
					<div className="px-2.5 max-[720px]:hidden">
						<DialogTitle>Settings</DialogTitle>
						<DialogDescription className="text-[12px] text-muted-foreground">
							{me.org.name}
						</DialogDescription>
					</div>
					{groups(me.org.name).map((group) => (
						<div
							key={group.id}
							className="flex flex-col gap-0.5 max-[720px]:flex-row"
						>
							<span className="px-2.5 pb-1 font-medium text-[12px] text-muted-foreground max-[720px]:hidden">
								{group.title}
							</span>
							{group.sections.map((item) => {
								const built = isSettingsSection(item.id);
								const current = item.id === section;
								return (
									<button
										key={item.id}
										type="button"
										disabled={!built}
										aria-current={current ? "page" : undefined}
										title={built ? undefined : "Not built yet"}
										onClick={() => {
											if (isSettingsSection(item.id)) onSection(item.id);
										}}
										className={cn(
											"flex h-[34px] shrink-0 items-center gap-2.5 rounded-row border-0 bg-transparent px-2.5 text-left text-[13px] text-soft-foreground",
											built
												? "cursor-pointer hover:bg-hover hover:text-foreground"
												: "cursor-not-allowed opacity-55 max-[720px]:hidden",
											current &&
												"bg-raised font-semibold text-foreground shadow-raised hover:bg-raised",
										)}
									>
										<Icon name={item.icon} size={15} strokeWidth={1.8} />
										{item.label}
									</button>
								);
							})}
						</div>
					))}
				</nav>
				<div className="min-h-0 flex-1 overflow-y-auto px-8 pt-8 pb-10 max-[720px]:px-4 max-[720px]:pt-4">
					<div className="mx-auto max-w-[760px]">{Page ? <Page /> : null}</div>
				</div>
			</DialogContent>
		</Dialog>
	);
}
