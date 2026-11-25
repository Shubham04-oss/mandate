import { cn } from "cn";
import type { ConversationState } from "#/lib/api/index.ts";

export type Mark =
	| "working"
	| "needs-you"
	| "done"
	| "failed"
	| "idle"
	| "info"
	| "error"
	| "lost";

const RADIUS = 9;
export const DASHES = 5;

/** Dash and gap along the ring, in viewBox units. Round caps eat into each gap, so the gap adds the stroke. */
export function ringDashes(stroke: number): { dash: number; gap: number } {
	const gap = (stroke < 2 ? 2 : 2.4) + stroke;
	return { dash: (2 * Math.PI * RADIUS) / DASHES - gap, gap };
}

const signs: Record<Exclude<Mark, "working" | "lost">, string> = {
	"needs-you": "M10 9.3v5.4M14 9.3v5.4",
	done: "m8.4 12.3 2.4 2.4 4.8-5",
	failed: "m9.5 9.5 5 5M14.5 9.5l-5 5",
	idle: "M9 12h6",
	info: "M12 11v5M12 8h.01",
	error: "M12 7.8v4.6M12 15.9h.01",
};

const tones: Record<Mark, string> = {
	working: "text-sea",
	// the orange belongs to what needs the person, nothing else
	"needs-you": "text-gold-deep",
	done: "text-ok-foreground",
	failed: "text-danger-foreground",
	idle: "text-muted-foreground",
	info: "text-sea",
	error: "text-danger-foreground",
	lost: "text-muted-foreground",
};

/**
 * A status as a small mark: a ring of five dashes with its sign inside, in the status's colour, and
 * nothing behind it. Working is a faint ring with one arc turning round it; lost touch is a broken
 * link. Named for screen readers when given a label.
 */
export function StatusMark({
	mark,
	label,
	size = 16,
	className,
}: {
	mark: Mark;
	label?: string;
	size?: number;
	className?: string;
}) {
	const stroke = size <= 15 ? 2 : 1.9;
	const { dash, gap } = ringDashes(stroke);
	const svg = (
		<svg
			aria-hidden="true"
			width={size}
			height={size}
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			strokeWidth={stroke}
			strokeLinecap="round"
			strokeLinejoin="round"
			data-mark={mark}
			className={cn(
				"flex-none",
				tones[mark],
				mark === "working" && "animate-spin",
				className,
			)}
		>
			{mark === "lost" ? (
				<path d="M9.5 14.5 8 16a3.5 3.5 0 0 1-5-5l1.5-1.5M14.5 9.5 16 8a3.5 3.5 0 0 1 5 5l-1.5 1.5M8 3.5v2M3.5 8h2M16 20.5v-2M20.5 16h-2" />
			) : (
				<circle
					cx="12"
					cy="12"
					r={RADIUS}
					strokeDasharray={`${dash.toFixed(2)} ${gap.toFixed(2)}`}
					// a dash sits centred at the top
					strokeDashoffset={(dash / 2).toFixed(2)}
					transform="rotate(-90 12 12)"
					strokeOpacity={mark === "working" ? 0.32 : undefined}
				/>
			)}
			{mark === "working" ? <path d="M12 3a9 9 0 0 1 9 9" /> : null}
			{mark !== "working" && mark !== "lost" ? <path d={signs[mark]} /> : null}
		</svg>
	);
	if (!label) return svg;
	return (
		<span role="img" aria-label={label} className="inline-flex flex-none">
			{svg}
		</span>
	);
}

/** Where a conversation stands: working, needs you, or couldn't start. Nothing once it's done. */
export function StateMark({ state }: { state: ConversationState }) {
	if (state === "working") return <StatusMark mark="working" label="Working" />;
	if (state === "waiting")
		return <StatusMark mark="needs-you" label="Needs you" />;
	if (state === "failed")
		return <StatusMark mark="failed" label="Couldn't start" />;
	return null;
}
