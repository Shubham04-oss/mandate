import type {
	Approval,
	ApprovalState,
	ConversationEvent,
	ConversationState,
	ToolStatus,
} from "#/lib/api/index.ts";

/** What the conversation reads like: your words, your agent's words, and anything that went wrong. */
export type Line =
	| { key: string; kind: "you" | "agent"; text: string }
	| { key: string; kind: "error"; text: string };

/** One thing your agent did, for the panel beside the conversation. */
export type Step = {
	key: string;
	tool: string;
	status: ToolStatus;
	/** Set when the call waited for someone's OK; its state follows the decision. */
	approval?: Approval & { state: ApprovalState };
};

export type View = {
	lines: Line[];
	steps: Step[];
	/** From the stream; null until it says something about the state. */
	state: ConversationState | null;
};

export const emptyView: View = { lines: [], steps: [], state: null };

const open = (s: Step) => s.status === "requested" || s.status === "running";

function lastIndex<T>(items: T[], match: (item: T) => boolean): number {
	for (let i = items.length - 1; i >= 0; i--)
		if (match(items[i] as T)) return i;
	return -1;
}

/** Folds one event into the view. Events arrive once each, in order (the stream drops repeats). */
export function applyEvent(view: View, e: ConversationEvent): View {
	const key = String(e.id);
	switch (e.kind) {
		case "message":
			return {
				...view,
				lines: [
					...view.lines,
					{ key, kind: e.role === "user" ? "you" : "agent", text: e.text },
				],
				state: e.role === "user" ? "working" : view.state,
			};
		case "tool": {
			// The core sends no call id: a status after "requested" belongs to the latest open call of
			// that tool.
			const at =
				e.status === "requested"
					? -1
					: lastIndex(view.steps, (s) => s.tool === e.name && open(s));
			const steps =
				at < 0
					? [...view.steps, { key, tool: e.name, status: e.status }]
					: view.steps.map((s, i) =>
							i === at ? { ...s, status: e.status } : s,
						);
			return {
				...view,
				steps,
				state: view.state === "waiting" ? "waiting" : "working",
			};
		}
		case "approval_wait": {
			const approval = e.approval;
			const at = lastIndex(
				view.steps,
				(s) => s.tool === approval.tool && open(s) && !s.approval,
			);
			const steps =
				at < 0
					? [
							...view.steps,
							{
								key,
								tool: approval.tool,
								status: "requested" as const,
								approval,
							},
						]
					: view.steps.map((s, i) => (i === at ? { ...s, approval } : s));
			return { ...view, steps, state: "waiting" };
		}
		case "approval_done":
			return {
				...view,
				steps: view.steps.map((s) =>
					s.approval?.id === e.approval_id
						? { ...s, approval: { ...s.approval, state: e.state } }
						: s,
				),
				state: "working",
			};
		case "error":
			return {
				...view,
				lines: [...view.lines, { key, kind: "error", text: e.message }],
			};
		case "done":
			return { ...view, state: "idle" };
	}
}

const tools: Record<
	string,
	{ doing: string; done: string; denied: string; failed: string }
> = {
	"web.read_page": {
		doing: "Reading a web page",
		done: "Read a web page",
		denied: "Didn't read a web page",
		failed: "Couldn't read a web page",
	},
	"web.submit_form": {
		doing: "Submitting a form",
		done: "Submitted a form",
		denied: "Didn't submit the form",
		failed: "Couldn't submit the form",
	},
};

/** A tool's plain name, for a step that hasn't run yet. */
export function toolTitle(tool: string): string {
	if (tool === "web.read_page") return "Read a web page";
	if (tool === "web.submit_form") return "Submit a form";
	return tool;
}

/** How a step reads in the panel, in your agent's voice. */
export function stepLabel(step: Pick<Step, "tool" | "status">): string {
	const t = tools[step.tool];
	if (!t) return step.tool;
	switch (step.status) {
		case "requested":
		case "running":
			return t.doing;
		case "completed":
			return t.done;
		case "denied":
			return t.denied;
		case "failed":
			return t.failed;
	}
}

/** The core's notice when its output guard holds a message back (core/src/guard.rs). */
export const WITHHELD = "This message was withheld by the output guard.";
