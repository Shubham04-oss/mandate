export type ToolStatus = "connected" | "connecting" | "expired" | "failed";

export type ToolGroup = {
	title: string;
	description: string;
	tools: CatalogTool[];
};

export type CatalogTool = {
	id: string;
	name: string;
	/** Key into the logo set; unknown keys fall back to the first letter. */
	logo: string;
	description: string;
	/** Fields the person types when the app connects with a key instead of signing in. */
	keyFields?: { id: string; label: string; help?: string; secret?: boolean }[];
};

export type ConnectedTool = {
	id: string;
	name: string;
	logo: string;
	status: ToolStatus;
	/** Who may use it through their agents: a value from `accessOptions`. */
	access: string;
	detail: string;
	events: { id: string; title: string; detail: string; on: boolean }[];
};

/** One line that says where the connections stand, e.g. "4 connected · 1 needs attention". */
export function summarizeConnections(
	tools: Pick<ConnectedTool, "status">[],
): string {
	const count = (match: (s: ToolStatus) => boolean) =>
		tools.filter((t) => match(t.status)).length;
	const connected = count((s) => s === "connected");
	const connecting = count((s) => s === "connecting");
	const attention = count((s) => s === "expired" || s === "failed");
	const parts = [`${connected} connected`];
	if (connecting) parts.push(`${connecting} connecting`);
	if (attention)
		parts.push(`${attention} ${attention === 1 ? "needs" : "need"} attention`);
	return parts.join(" · ");
}

/**
 * Narrows the catalog to apps whose name or group matches. One letter matches almost everything, so
 * the full catalog shows until there are two.
 */
export function searchCatalog(groups: ToolGroup[], query: string): ToolGroup[] {
	const q = query.trim().toLowerCase();
	if (q.length < 2) return groups;
	return groups
		.map((group) =>
			group.title.toLowerCase().includes(q)
				? group
				: {
						...group,
						tools: group.tools.filter((t) => t.name.toLowerCase().includes(q)),
					},
		)
		.filter((group) => group.tools.length > 0);
}
