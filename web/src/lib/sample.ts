import type { ConnectedTool, ToolGroup } from "#/lib/tools.ts";

// ponytail: sample people and organizations so the screens render as designed. Each value will come
// from the core (the person's email, their invitation, their organizations) once the forms are wired.
export const sample = {
	email: "ada@northstar.io",
	me: { name: "Ada Brennan", avatar: "/avatars/ada.svg" },
	agent: { avatar: "/avatars/agent.svg" },
	org: {
		name: "Northstar",
		initial: "N",
		summary: "12 people · 9 agents",
		faces: [
			"/avatars/raya.svg",
			"/avatars/priya.svg",
			"/avatars/rohan.svg",
			"/avatars/member.svg",
		],
		facesLine: "Raya, Priya, Rohan and 9 others",
		inviter: { name: "Raya Patel", avatar: "/avatars/raya.svg" },
	},
	orgs: [
		{
			id: "northstar",
			name: "Northstar",
			initial: "N",
			summary: "12 people · 9 agents",
		},
		{
			id: "harbourline",
			name: "Harbourline",
			initial: "H",
			summary: "48 people · 21 agents",
		},
		{
			id: "tidewater",
			name: "Tidewater Labs",
			initial: "T",
			summary: "6 people · 3 agents",
		},
	],
	mfa: { qr: "/sample/mfa-qr.svg", secret: "JBSW Y3DP EHPK 3PXP" },
	// Home
	needsYou: [
		{
			id: "q3",
			avatar: "/avatars/priya.svg",
			who: "Priya Shah",
			what: "Q3 revenue for the EU",
		},
		{
			id: "mail",
			avatar: "/avatars/agent.svg",
			who: "Support bot",
			what: "Email to dana@acme.com",
		},
		{
			id: "ops",
			avatar: "/avatars/rohan.svg",
			who: "Rohan Mehta",
			what: "Ops bot in #incidents",
		},
	],
	working: [
		{
			id: "expenses",
			kind: "live",
			title: "Filing October expenses",
			detail: "Live in Expensify",
			when: "3 of 5",
		},
		{
			id: "launch",
			kind: "agent",
			title: "Summarising #launch",
			detail: "Slack",
			when: "2 min",
		},
		{
			id: "weekly",
			kind: "scheduled",
			title: "Weekly report for Raya",
			detail: "Scheduled",
			when: "Mon 9:00",
		},
	],
	recent: [
		{
			id: "kenji",
			kind: "done",
			title: "Answered Kenji",
			detail: "Release date",
			when: "10:12",
		},
		{
			id: "crm",
			kind: "refused",
			title: "Refused a CRM export",
			detail: "Policy refused it",
			when: "09:40",
		},
		{
			id: "ops-end",
			kind: "expired",
			title: "Ops bot's access ended",
			detail: "1-hour grant on #incidents",
			when: "09:00",
		},
	],
	handledToday: 6,
	// Settings → Tools
	tools: {
		accessOptions: [
			{ value: "everyone", label: "Everyone" },
			{ value: "you", label: "Only you" },
			{ value: "engineering", label: "Engineering" },
			{ value: "finance", label: "Finance" },
		],
		connected: [
			{
				id: "slack",
				name: "Slack",
				logo: "slack",
				status: "connected",
				access: "everyone",
				detail: "14 channels",
				events: [
					{
						id: "mention",
						title: "You're mentioned",
						detail: "Live · 212 this week",
						on: true,
					},
					{
						id: "dm",
						title: "A direct message arrives",
						detail: "Live · 48 this week",
						on: true,
					},
				],
			},
			{
				id: "github",
				name: "GitHub",
				logo: "github",
				status: "connected",
				access: "engineering",
				detail: "3 repos",
				events: [
					{
						id: "push",
						title: "Push",
						detail: "Live · 128 deliveries",
						on: true,
					},
					{
						id: "pr",
						title: "Pull request opened",
						detail: "Live · 37 deliveries",
						on: true,
					},
					{
						id: "issue",
						title: "New issue",
						detail: "Turned off after repeated failures",
						on: false,
					},
				],
			},
			{
				id: "notion",
				name: "Notion",
				logo: "notion",
				status: "connected",
				access: "everyone",
				detail: "38 pages",
				events: [],
			},
			{
				id: "gmail",
				name: "Gmail",
				logo: "gmail",
				status: "connecting",
				access: "you",
				detail: "Waiting for Google",
				events: [],
			},
			{
				id: "drive",
				name: "Google Drive",
				logo: "googledrive",
				status: "connected",
				access: "everyone",
				detail: "52 files",
				events: [],
			},
			{
				id: "linear",
				name: "Linear",
				logo: "linear",
				status: "expired",
				access: "engineering",
				detail: "Sign-in expired",
				events: [],
			},
		] satisfies ConnectedTool[] as ConnectedTool[],
		catalog: [
			{
				title: "Communication",
				description: "Conversations, meetings and calendars.",
				tools: [
					{
						id: "discord",
						name: "Discord",
						logo: "discord",
						description: "Read and post in your servers' channels.",
					},
					{
						id: "zoom",
						name: "Zoom",
						logo: "zoom",
						description: "Schedule meetings and read their summaries.",
					},
					{
						id: "gcal",
						name: "Google Calendar",
						logo: "googlecalendar",
						description: "See free time and book meetings.",
					},
					{
						id: "intercom",
						name: "Intercom",
						logo: "intercom",
						description: "Read and answer customer conversations.",
					},
				],
			},
			{
				title: "Planning & projects",
				description: "Keep work moving across every team.",
				tools: [
					{
						id: "asana",
						name: "Asana",
						logo: "asana",
						description: "Create and update tasks and projects.",
					},
					{
						id: "trello",
						name: "Trello",
						logo: "trello",
						description: "Move cards and keep boards current.",
					},
					{
						id: "jira",
						name: "Jira",
						logo: "jira",
						description: "Find, create and update issues.",
						keyFields: [
							{
								id: "token",
								label: "API token",
								help: "Create one at id.atlassian.com → Security → API tokens.",
								secret: true,
							},
							{ id: "email", label: "Atlassian email" },
							{
								id: "site",
								label: "Jira site",
								help: "The part before .atlassian.net.",
							},
						],
					},
				],
			},
			{
				title: "Files & knowledge",
				description: "Where your team's documents live.",
				tools: [
					{
						id: "dropbox",
						name: "Dropbox",
						logo: "dropbox",
						description: "Find and read files and folders.",
					},
					{
						id: "airtable",
						name: "Airtable",
						logo: "airtable",
						description: "Read and update bases and records.",
					},
					{
						id: "confluence",
						name: "Confluence",
						logo: "confluence",
						description: "Search and read pages and spaces.",
					},
				],
			},
			{
				title: "Build & business",
				description: "Code, design, payments and customers.",
				tools: [
					{
						id: "figma",
						name: "Figma",
						logo: "figma",
						description: "Read files, frames and comments.",
					},
					{
						id: "stripe",
						name: "Stripe",
						logo: "stripe",
						description: "Look up customers, payments and invoices.",
						keyFields: [
							{
								id: "key",
								label: "Restricted key",
								help: "Use a read-only restricted key from the Stripe dashboard.",
								secret: true,
							},
						],
					},
					{
						id: "hubspot",
						name: "HubSpot",
						logo: "hubspot",
						description: "Read and update contacts and deals.",
					},
					{
						id: "zendesk",
						name: "Zendesk",
						logo: "zendesk",
						description: "Read and answer support tickets.",
					},
				],
			},
		] satisfies ToolGroup[] as ToolGroup[],
	},
};
