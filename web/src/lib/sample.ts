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
};
