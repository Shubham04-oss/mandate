import { cn } from "cn";
import { Icon, type IconName } from "#/components/icons/icon.tsx";
import { ToolsSection } from "#/components/settings/tools-section.tsx";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogTitle,
} from "#/components/ui/dialog.tsx";
import { sample } from "#/lib/sample.ts";

type Section = { id: string; label: string; icon: IconName; built?: boolean };

// ponytail: only Tools is built; the other sections show the shape settings will take.
const groups: { title: string; sections: Section[] }[] = [
	{
		title: "You",
		sections: [
			{ id: "account", label: "Account", icon: "user" },
			{ id: "agent", label: "Your agent", icon: "bot" },
			{ id: "notifications", label: "Notifications", icon: "mail" },
		],
	},
	{
		title: sample.org.name,
		sections: [
			{ id: "organization", label: "Organization", icon: "building" },
			{ id: "people", label: "People", icon: "user" },
			{ id: "teams", label: "Teams", icon: "user" },
		],
	},
	{
		title: "Connections",
		sections: [{ id: "tools", label: "Tools", icon: "link", built: true }],
	},
];

/** Settings open over the app rather than as a page, so the person keeps their place. */
export function SettingsDialog({
	open,
	onOpenChange,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="h-[min(760px,calc(100dvh-2rem))] w-[min(1080px,calc(100%-2rem))] max-w-none rounded-3xl max-[720px]:h-dvh max-[720px]:w-full max-[720px]:max-w-full max-[720px]:rounded-none min-[721px]:flex-row">
				<nav
					aria-label="Settings"
					className="flex flex-none flex-col gap-4 bg-background px-3 pt-5 pb-3 max-[720px]:flex-row max-[720px]:items-center max-[720px]:gap-1 max-[720px]:overflow-x-auto max-[720px]:pt-3 max-[720px]:pr-14 min-[721px]:w-56"
				>
					<div className="px-2.5 max-[720px]:hidden">
						<DialogTitle>Settings</DialogTitle>
						<DialogDescription className="text-[12px] text-muted-foreground">
							{sample.org.name}
						</DialogDescription>
					</div>
					{groups.map((group) => (
						<div
							key={group.title}
							className="flex flex-col gap-0.5 max-[720px]:flex-row"
						>
							<span className="px-2.5 pb-1 font-semibold text-[11px] text-muted-foreground uppercase tracking-[0.04em] max-[720px]:hidden">
								{group.title}
							</span>
							{group.sections.map((section) => (
								<button
									key={section.id}
									type="button"
									disabled={!section.built}
									aria-current={section.built ? "page" : undefined}
									title={section.built ? undefined : "Not built yet"}
									className={cn(
										"flex h-[34px] shrink-0 items-center gap-2.5 rounded-row border-0 bg-transparent px-2.5 text-left text-[13px] text-soft-foreground",
										section.built
											? "cursor-pointer bg-raised font-semibold text-foreground shadow-raised"
											: "cursor-not-allowed opacity-55 max-[720px]:hidden",
									)}
								>
									<Icon name={section.icon} size={15} strokeWidth={1.8} />
									{section.label}
								</button>
							))}
						</div>
					))}
				</nav>
				<div className="min-h-0 flex-1 overflow-y-auto px-8 pt-8 pb-10 max-[720px]:px-4 max-[720px]:pt-4">
					<div className="mx-auto max-w-[760px]">
						<ToolsSection />
					</div>
				</div>
			</DialogContent>
		</Dialog>
	);
}
