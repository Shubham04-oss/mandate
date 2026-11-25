import { useRouter } from "@tanstack/react-router";
import { useState } from "react";
import { StatusMark } from "#/components/agent/marks.tsx";
import { Button } from "#/components/ui/button.tsx";
import { ApiError, type Approval, api } from "#/lib/api/index.ts";
import { type Step, stepLabel, toolTitle } from "#/lib/conversation.ts";
import { clock, formatTime, usePref } from "#/lib/preferences.ts";

/** Beside the conversation: what your agent did and is doing, with any OK it needs. */
export function ActivityPanel({ steps }: { steps: Step[] }) {
	return (
		<aside
			aria-labelledby="work-h"
			className="mt-11 flex min-w-0 flex-[1_1_300px] flex-col gap-5 self-start rounded-[14px] bg-raised p-[22px] shadow-[inset_0_0_0_1px_var(--border-quiet)] lg:sticky lg:top-12 lg:max-w-[380px]"
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
	const waiting = step.approval?.state === "pending";
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
	return (
		<span className="text-[12px] text-muted-foreground">
			{approval.state === "pending"
				? `Needs your OK · waits until ${formatTime(approval.expires_at, timeFormat)}`
				: (decided[approval.state] ?? approval.state)}
		</span>
	);
}

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

function Decide({ approval }: { approval: Approval }) {
	const router = useRouter();
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const decide = async (approve: boolean) => {
		setBusy(true);
		setError(null);
		try {
			await (approve ? api.approvals.approve : api.approvals.deny)(approval.id);
			// The stream reports the decision; the rail's count needs the inbox again.
			void router.invalidate();
		} catch (e) {
			setBusy(false);
			setError(
				e instanceof ApiError && e.code === "conflict"
					? "Someone already decided this."
					: e instanceof ApiError
						? e.message
						: "Something went wrong. Try again.",
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
			{approval.can_decide ? (
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
