import {
	type Agent,
	ApiError,
	type Grant,
	type GrantPrincipal,
	type Me,
	type Person,
	type Team,
} from "#/lib/api/index.ts";

/**
 * The core's tools in v1 (core/src/tools/browser.rs).
 * ponytail: the core has no endpoint that lists its tools, so the web app names them; noted for the
 * lead.
 */
export const catalog = [
	{
		id: "web.read_page",
		title: "Read a web page",
		about:
			"Opens a page and reads it. Pages on your allowed sites open straight away; any other page asks first.",
	},
	{
		id: "web.submit_form",
		title: "Submit a form",
		about: "Fills in and sends a form on a site. It always asks first.",
	},
] as const;

export const MAX_DAYS = 90;
export const expiryChoices = [1, 7, 30, 90] as const;

/** When a grant made now for `days` ends, never past the core's 90-day limit. */
export function expiresAt(days: number, now = new Date()): string {
	const d = Math.min(Math.max(Math.round(days), 1), MAX_DAYS);
	// A minute short of the limit, so a clock a little ahead of the core's isn't refused.
	const margin = d === MAX_DAYS ? 60_000 : 0;
	return new Date(now.getTime() + d * 86_400_000 - margin).toISOString();
}

export type Access = "manage" | "not_admin";

/**
 * Whether to show the grant controls. Only admins see them, and a 403 from the core means the same.
 * This only shapes the page: the core decides every grant change on its own.
 */
export function grantsAccess(
	me: Pick<Me, "is_admin">,
	error?: unknown,
): Access {
	if (!me.is_admin) return "not_admin";
	if (error instanceof ApiError && error.status === 403) return "not_admin";
	return "manage";
}

/** One tool's grants: live ones first, soonest to end at the top, then the expired. */
export function forTool(grants: Grant[], tool: string): Grant[] {
	return grants
		.filter((g) => g.tool === tool)
		.sort((a, b) =>
			a.state === b.state
				? a.expires_at.localeCompare(b.expires_at)
				: a.state === "active"
					? -1
					: 1,
		);
}

/** Who a grant is for, by kind and id; a grant names its principal by id only. */
export type Directory = Map<string, string>;

export const principalKey = (p: GrantPrincipal) => `${p.kind}:${p.id}`;

export function principalName(p: GrantPrincipal, names: Directory): string {
	return names.get(principalKey(p)) ?? p.id;
}

/** Someone a grant can be for: a person, a team, or an agent. */
export type Choice = {
	principal: GrantPrincipal;
	name: string;
	detail: string;
};

export function grantChoices(
	members: Person[],
	teams: Team[],
	agents: Agent[],
): Choice[] {
	return [
		...members.map((p) => ({
			principal: { kind: "member" as const, id: p.id },
			name: p.name,
			// A member who left has no email.
			detail: p.email ?? "",
		})),
		...teams.map((t) => ({
			principal: { kind: "team" as const, id: t.id },
			name: t.name,
			detail: `${t.members} ${t.members === 1 ? "person" : "people"}`,
		})),
		...agents.map((a) => ({
			principal: { kind: "agent" as const, id: a.id },
			name: a.name,
			detail: "Agent",
		})),
	];
}

/** The choices matching a search by name or detail; the one already picked always stays. */
export function searchChoices(
	choices: Choice[],
	query: string,
	picked: string,
): Choice[] {
	const q = query.trim().toLowerCase();
	return choices.filter(
		(c) =>
			principalKey(c.principal) === picked ||
			!q ||
			(c.name ?? "").toLowerCase().includes(q) ||
			(c.detail ?? "").toLowerCase().includes(q),
	);
}

export const kindLabel: Record<GrantPrincipal["kind"], string> = {
	member: "Person",
	team: "Team",
	agent: "Agent",
};
