import { Decide } from "#/components/agent/decide.tsx";
import { StatusMark } from "#/components/icons/status-mark.tsx";
import type { Approval } from "#/lib/api/index.ts";
import {
	approvalState,
	type Step,
	stepLabel,
	toolTitle,
} from "#/lib/conversation.ts";
import { clock, formatTime, usePref } from "#/lib/preferences.ts";

/** Beside the conversation: what your agent did and is doing, with any OK it needs. */
export function ActivityPanel({ steps }: { steps: Step[] }) {
	return (
		<aside
			aria-labelledby="work-h"
			className="mt-11 flex min-w-0 flex-[1_1_300px] flex-col gap-5 self-start rim rounded-window bg-raised p-[22px] lg:sticky lg:top-12 lg:max-w-[380px]"
		>
			<h2 id="work-h" className="eyebrow">
				What your agent did
			</h2>
			{steps.length === 0 ? (
				<p className="m-0 text-[13px] text-muted-foreground leading-normal">
					Nothing yet. Each step shows here as your agent works.
				</p>
			) : (
				<ol className="m-0 flex list-none flex-col gap-[18px] p-0">
					{steps.map((step) => (
						<StepItem key={step.key} step={step} />
					))}
				</ol>
			)}
			<p className="m-0 text-[12px] text-muted-foreground leading-normal">
				Your agent reads pages with your access. Anything that changes something
				waits here for your OK.
			</p>
		</aside>
	);
}

const decided: Record<string, string> = {
	approved: "You approved it",
	used: "Approved",
	denied: "Denied",
	expired: "No one decided in time",
};

function StepItem({ step }: { step: Step }) {
	const waiting = step.approval
		? approvalState(step.approval) === "pending"
		: false;
	return (
		<li className="grid grid-cols-[20px_minmax(0,1fr)] gap-x-3">
			<span className="mt-px grid size-5 place-items-center">
				<StepIcon step={step} waiting={waiting} />
			</span>
			<div className="flex min-w-0 flex-col gap-3.5">
				<div className="flex flex-col gap-0.5">
					<span
						className={
							waiting ? "font-semibold text-[13.5px]" : "text-[13.5px]"
						}
					>
						{waiting ? toolTitle(step.tool) : stepLabel(step)}
					</span>
					{step.approval ? <ApprovalNote approval={step.approval} /> : null}
				</div>
				{waiting && step.approval ? <Decide approval={step.approval} /> : null}
			</div>
		</li>
	);
}

function StepIcon({ step, waiting }: { step: Step; waiting: boolean }) {
	if (waiting)
		return <StatusMark mark="needs-you" label="Needs your OK" size={15} />;
	if (step.status === "requested" || step.status === "running")
		return <StatusMark mark="working" label="Under way" size={15} />;
	if (step.status === "completed")
		return <StatusMark mark="done" label="Done" size={15} />;
	return (
		<StatusMark
			mark="failed"
			label={step.status === "denied" ? "Not done" : "Failed"}
			size={15}
			className="text-muted-foreground"
		/>
	);
}

function ApprovalNote({ approval }: { approval: Approval }) {
	const timeFormat = usePref(clock);
	const state = approvalState(approval);
	return (
		<span className="text-[12px] text-muted-foreground">
			{state === "pending"
				? `Needs your OK · waits until ${formatTime(approval.expires_at, timeFormat)}`
				: (decided[state] ?? state)}
		</span>
	);
}
