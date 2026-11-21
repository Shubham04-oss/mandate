import { cn } from "cn";
import {
	type SimpleIcon,
	siAirtable,
	siAsana,
	siConfluence,
	siDiscord,
	siDropbox,
	siFigma,
	siGithub,
	siGmail,
	siGooglecalendar,
	siGoogledrive,
	siHubspot,
	siIntercom,
	siJira,
	siLinear,
	siNotion,
	siStripe,
	siTrello,
	siZendesk,
	siZoom,
} from "simple-icons";
import { SlackMark } from "#/components/icons/brand-marks.tsx";

// Each app's own mark from simple-icons, in its brand colour. Slack isn't in that set.
const marks: Record<string, SimpleIcon> = {
	airtable: siAirtable,
	asana: siAsana,
	confluence: siConfluence,
	discord: siDiscord,
	dropbox: siDropbox,
	figma: siFigma,
	github: siGithub,
	gmail: siGmail,
	googlecalendar: siGooglecalendar,
	googledrive: siGoogledrive,
	hubspot: siHubspot,
	intercom: siIntercom,
	jira: siJira,
	linear: siLinear,
	notion: siNotion,
	stripe: siStripe,
	trello: siTrello,
	zendesk: siZendesk,
	zoom: siZoom,
};

/** An app's mark on a white tile, the same in both themes; unknown apps show their first letter. */
export function AppLogo({
	logo,
	name,
	size = 34,
	className,
}: {
	logo: string;
	name: string;
	size?: number;
	className?: string;
}) {
	const icon = marks[logo];
	const mark = Math.round(size * 0.52);
	return (
		<span
			aria-hidden="true"
			className={cn(
				"grid shrink-0 place-items-center rounded-[10px] bg-white shadow-[inset_0_0_0_1px_var(--border-quiet)] dark:shadow-none",
				className,
			)}
			style={{ width: size, height: size }}
		>
			{logo === "slack" ? (
				<SlackMark size={mark} />
			) : icon ? (
				<svg
					width={mark}
					height={mark}
					viewBox="0 0 24 24"
					fill={`#${icon.hex}`}
					aria-hidden="true"
				>
					<path d={icon.path} />
				</svg>
			) : (
				<span
					className="font-bold text-[#222222]"
					style={{ fontSize: Math.round(size * 0.45) }}
				>
					{name[0]}
				</span>
			)}
		</span>
	);
}
