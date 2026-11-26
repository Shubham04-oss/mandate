import { useRouter } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import { Button } from "#/components/ui/button.tsx";
import { ApiError, type Approval, api } from "#/lib/api/index.ts";

/** An approval's arguments as plain text; the page first, under a name people use. */
function details(args: Record<string, unknown>): [string, string][] {
	const shown = (v: unknown) =>
		typeof v === "string"
			? v
			: typeof v === "object"
				? JSON.stringify(v)
				: String(v);
	// A page reads better as help.acme.com/upgrade than with its scheme in front.
	const page = (v: unknown) =>
		typeof v === "string" ? v.replace(/^https?:\/\//, "") : shown(v);
	const rows = Object.entries(args).map(([k, v]): [string, string] =>
		k === "url" ? ["Page", page(v)] : [k, shown(v)],
	);
	return rows.sort(([a], [b]) => (a === "Page" ? -1 : b === "Page" ? 1 : 0));
}

/** What a waiting approval would do, and Approve / Deny for someone who can decide it. */
export function Decide({
	approval,
	onDecided,
}: {
	approval: Approval;
	/** Told what was said once the decision holds, e.g. to move focus when this card goes. */
	onDecided?: (outcome: string) => void;
}) {
	const router = useRouter();
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);
	// Said at once, so the buttons can't be pressed again before the stream or the reload catches up.
	const [outcome, setOutcome] = useState<string | null>(null);
	const said = useRef<HTMLOutputElement>(null);
	// The buttons are gone, so focus goes to what replaced them rather than to the top of the page.
	useEffect(() => {
		if (outcome) said.current?.focus();
	}, [outcome]);

	const decide = async (approve: boolean) => {
		setBusy(true);
		setError(null);
		try {
			await (approve ? api.approvals.approve : api.approvals.deny)(approval.id);
			setBusy(false);
			const text = approve ? "You approved it." : "You denied it.";
			setOutcome(text);
			onDecided?.(text);
			// The rail's count, the approvals page and Home all need the inbox again.
			void router.invalidate();
		} catch (e) {
			setBusy(false);
			// The core's 409 says why: already decided, expired, or the agent is disabled.
			setError(
				e instanceof ApiError ? e.message : "Something went wrong. Try again.",
			);
		}
	};

	return (
		<>
			<dl className="m-0 grid grid-cols-[76px_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[12.5px] leading-normal">
				{details(approval.args).map(([k, v]) => (
					<div key={k} className="contents">
						<dt className="text-muted-foreground">{k}</dt>
						<dd className="m-0 [overflow-wrap:anywhere]">{v}</dd>
					</div>
				))}
			</dl>
			{outcome ? (
				<output
					ref={said}
					tabIndex={-1}
					className="m-0 block text-[12px] text-muted-foreground"
				>
					{outcome}
				</output>
			) : approval.can_decide ? (
				<div className="flex flex-wrap gap-2">
					<Button disabled={busy} onClick={() => decide(true)}>
						Approve
					</Button>
					<Button
						variant="outline"
						disabled={busy}
						onClick={() => decide(false)}
					>
						Deny
					</Button>
				</div>
			) : (
				<p className="m-0 text-[12px] text-muted-foreground">
					Someone who can decide has been asked.
				</p>
			)}
			{error ? (
				<p role="alert" className="m-0 text-[12px] text-danger-foreground">
					{error}
				</p>
			) : null}
		</>
	);
}
