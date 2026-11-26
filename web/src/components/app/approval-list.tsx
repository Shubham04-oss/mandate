import { Decide } from "#/components/agent/decide.tsx";
import { StatusMark } from "#/components/icons/status-mark.tsx";
import { agentName, describeApproval } from "#/lib/activity.ts";
import type { Approval } from "#/lib/api/index.ts";
import { clock, formatTime, formatWhen, usePref } from "#/lib/preferences.ts";

/** Everything waiting: what the person can decide, and their own requests waiting on someone else. */
export function ApprovalList({ approvals }: { approvals: Approval[] }) {
	return (
		<div className="flex flex-1 flex-col items-center px-4 pt-10 pb-10">
			<div className="flex w-full max-w-[620px] flex-col gap-3">
				{approvals.length === 0 ? (
					<p className="m-0 py-10 text-center text-[13px] text-muted-foreground">
						Nothing is waiting for an OK.
					</p>
				) : (
					<ul
						aria-label="Waiting for an OK"
						className="m-0 flex list-none flex-col gap-3 p-0"
					>
						{approvals.map((a) => (
							<ApprovalCard key={a.id} approval={a} />
						))}
					</ul>
				)}
			</div>
		</div>
	);
}

// ponytail: no link to the conversation; the core's Approval view doesn't name it yet.
function ApprovalCard({ approval }: { approval: Approval }) {
	const timeFormat = usePref(clock);
	return (
		<li className="rim flex flex-col gap-3.5 rounded-window bg-raised p-[18px]">
			<div className="grid grid-cols-[20px_minmax(0,1fr)] gap-x-3">
				<span className="mt-px grid size-5 place-items-center">
					<StatusMark mark="needs-you" size={15} />
				</span>
				<div className="flex min-w-0 flex-col gap-0.5">
					<span className="font-semibold text-[13.5px]">
						{describeApproval(approval)}
					</span>
					<span className="text-[12px] text-muted-foreground">
						{agentName(approval.agent)} · asked by {approval.requested_by.name} ·{" "}
						{formatWhen(approval.requested_at, timeFormat)} · waits until{" "}
						{formatTime(approval.expires_at, timeFormat)}
					</span>
				</div>
			</div>
			<Decide approval={approval} />
		</li>
	);
}
