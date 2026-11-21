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
};
