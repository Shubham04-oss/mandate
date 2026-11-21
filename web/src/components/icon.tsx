import {
	Activity,
	Bot,
	Inbox,
	type LucideProps,
	Search,
	Settings,
	Users,
} from "lucide-react";

// The one place that names an icon set, so swapping in our own icons is a change to this file.
const icons = {
	activity: Activity,
	agents: Bot,
	inbox: Inbox,
	search: Search,
	settings: Settings,
	teams: Users,
};

export type IconName = keyof typeof icons;

export function Icon({ name, ...props }: LucideProps & { name: IconName }) {
	const Glyph = icons[name];
	return <Glyph aria-hidden="true" {...props} />;
}
