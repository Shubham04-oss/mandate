import { StatusMark } from "#/components/icons/status-mark.tsx";
import type { ConversationState } from "#/lib/api/index.ts";

/** Where a conversation stands: working, needs you, expired or couldn't start. Nothing once it's done. */
export function StateMark({ state }: { state: ConversationState }) {
	if (state === "working") return <StatusMark mark="working" label="Working" />;
	if (state === "waiting")
		return <StatusMark mark="needs-you" label="Needs you" />;
	// waited for an OK nobody gave in time; quiet, since only Stop is left to do
	if (state === "expired") return <StatusMark mark="idle" label="Expired" />;
	if (state === "failed")
		return <StatusMark mark="failed" label="Couldn't start" />;
	return null;
}
