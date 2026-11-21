import type { LinkProps } from "@tanstack/react-router";
import type { IconName } from "#/components/icon.tsx";

export type NavItem = { title: string; to: LinkProps["to"]; icon: IconName };

// Shared by the sidebar and the command palette so the two never disagree.
export const mainNav: NavItem[] = [
	{ title: "Inbox", to: "/", icon: "inbox" },
	{ title: "Agents", to: "/agents", icon: "agents" },
	{ title: "Teams", to: "/teams", icon: "teams" },
	{ title: "Activity", to: "/activity", icon: "activity" },
];

export const footerNav: NavItem[] = [
	{ title: "Settings", to: "/settings", icon: "settings" },
];
