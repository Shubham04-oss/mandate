import { Link } from "@tanstack/react-router";
import { useEffect, useRef, useState } from "react";
import { Decide } from "#/components/agent/decide.tsx";
import { StatusMark } from "#/components/icons/status-mark.tsx";
import { agentName, describeApproval } from "#/lib/activity.ts";
import type { Approval } from "#/lib/api/index.ts";
import { clock, formatTime, formatWhen, usePref } from "#/lib/preferences.ts";

/** Everything waiting: what the person can decide, and their own requests waiting on someone else. */
export function ApprovalList({
	approvals,
	me,
}: {
	approvals: Approval[];
	/** The signed-in person's id: only the asker can open the conversation (the core 404s others). */
	me: string;
}) {
	const heading = useRef<HTMLHeadingElement>(null);
	const cards = useRef(new Map<string, HTMLLIElement>());
	// Where the decided card was, until the reloaded list has dropped it.
	const [gone, setGone] = useState<{ id: string; at: number } | null>(null);
	const [said, setSaid] = useState("");

	// A decided card leaves the list; focus goes to the one that took its place, or the heading.
	useEffect(() => {
		if (!gone || approvals.some((a) => a.id === gone.id)) return;
		const next = approvals[Math.min(gone.at, approvals.length - 1)];
		(next ? cards.current.get(next.id) : heading.current)?.focus();
		setGone(null);
	}, [approvals, gone]);

	return (
		<div className="flex flex-1 flex-col items-center px-4 pt-10 pb-10">
			<div className="flex w-full max-w-[620px] flex-col gap-3">
				<h2
					id="approvals-h"
					ref={heading}
					tabIndex={-1}
					className="eyebrow outline-none"
				>
					Waiting for an OK
				</h2>
				{/* outlives the card it came from, so the decision is still announced */}
				<output className="sr-only">{said}</output>
				{approvals.length === 0 ? (
					<p className="m-0 py-10 text-center text-[13px] text-muted-foreground">
						Nothing is waiting for an OK.
					</p>
				) : (
					<ul
						aria-labelledby="approvals-h"
						className="m-0 flex list-none flex-col gap-3 p-0"
					>
						{approvals.map((a, at) => (
							<ApprovalCard
								key={a.id}
								approval={a}
								mine={a.requested_by.id === me}
								cardRef={(el) => {
									if (el) cards.current.set(a.id, el);
									else cards.current.delete(a.id);
								}}
								onDecided={(outcome) => {
									setSaid(outcome);
									setGone({ id: a.id, at });
								}}
							/>
						))}
					</ul>
				)}
			</div>
		</div>
	);
}

function ApprovalCard({
	approval,
	mine,
	cardRef,
	onDecided,
}: {
	approval: Approval;
	mine: boolean;
	cardRef: (el: HTMLLIElement | null) => void;
	onDecided: (outcome: string) => void;
}) {
	const timeFormat = usePref(clock);
	return (
		<li
			ref={cardRef}
			tabIndex={-1}
			aria-label={describeApproval(approval)}
			className="rim flex flex-col gap-3.5 rounded-window bg-raised p-[18px]"
		>
			<div className="grid grid-cols-[20px_minmax(0,1fr)] gap-x-3">
				<span className="mt-px grid size-5 place-items-center">
					<StatusMark mark="needs-you" size={15} />
				</span>
				<div className="flex min-w-0 flex-col gap-0.5">
					<span className="font-semibold text-[13.5px]">
						{describeApproval(approval)}
					</span>
					<span className="text-[12px] text-muted-foreground">
						{agentName(approval.agent)} · asked by {approval.requested_by.name}{" "}
						· {formatWhen(approval.requested_at, timeFormat)} · waits until{" "}
						{formatTime(approval.expires_at, timeFormat)}
					</span>
					{mine && approval.conversation && (
						<Link
							to="/agent/$conversationId"
							params={{ conversationId: approval.conversation }}
							className="w-fit text-[12px] underline underline-offset-2"
						>
							Open the conversation
						</Link>
					)}
				</div>
			</div>
			<Decide approval={approval} onDecided={onDecided} />
		</li>
	);
}
