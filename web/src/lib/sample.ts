// Made-up people for the /preview pages only; real screens get everything from the core.
import ada from "#/dev/avatars/ada.svg?url";
import agent from "#/dev/avatars/agent.svg?url";
import member from "#/dev/avatars/member.svg?url";
import priya from "#/dev/avatars/priya.svg?url";
import raya from "#/dev/avatars/raya.svg?url";
import rohan from "#/dev/avatars/rohan.svg?url";

export const sample = {
	me: { name: "Ada Brennan", avatar: ada },
	agent: { avatar: agent },
	member: { avatar: member },
	org: {
		name: "Northstar",
		faces: [raya, priya, rohan, member],
		inviter: { name: "Raya Patel", avatar: raya },
	},
};
