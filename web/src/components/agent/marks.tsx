import { StatusMark } from "#/components/icons/status-mark.tsx";
import type { ConversationState } from "#/lib/api/index.ts";

/** Where a conversation stands: working, needs you, or couldn't start. Nothing once it's done. */
export function StateMark({ state }: { state: ConversationState }) {
	if (state === "working") return <StatusMark mark="working" label="Working" />;
	if (state === "waiting")
		return <StatusMark mark="needs-you" label="Needs you" />;
	if (state === "failed")
		return <StatusMark mark="failed" label="Couldn't start" />;
	return null;
}
