import { cn } from "cn";
import type { ConversationState } from "#/lib/api/index.ts";

/** A small turning arc: something is under way. Still when motion is reduced. */
export function Spinner({ className }: { className?: string }) {
	return (
		<svg
			aria-hidden="true"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			strokeWidth={2.6}
			strokeLinecap="round"
			className={cn("size-3 flex-none animate-spin", className)}
		>
			<path d="M12 3a9 9 0 1 0 9 9" />
		</svg>
	);
}

/**
 * Where a conversation stands, without a badge: a spinner while your agent works, the orange dot
 * when it needs you, and nothing once it's done.
 */
export function StateMark({ state }: { state: ConversationState }) {
	if (state === "working")
		return (
			<span role="img" aria-label="Working" className="text-muted-foreground">
				<Spinner />
			</span>
		);
	if (state === "waiting")
		return (
			<i
				role="img"
				aria-label="Needs you"
				className="status-dot bg-gold-deep"
			/>
		);
	if (state === "failed")
		return (
			<i
				role="img"
				aria-label="Couldn't start"
				className="status-dot bg-destructive"
			/>
		);
	return null;
}
