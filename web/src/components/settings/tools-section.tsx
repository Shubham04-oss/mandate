import { getRouteApi } from "@tanstack/react-router";
import { type FormEvent, useCallback, useEffect, useState } from "react";
import { Icon } from "#/components/icons/icon.tsx";
import { StatusMark } from "#/components/icons/status-mark.tsx";
import { SettingsHeader } from "#/components/settings/parts.tsx";
import {
	AlertDialog,
	AlertDialogAction,
	AlertDialogCancel,
	AlertDialogContent,
	AlertDialogDescription,
	AlertDialogFooter,
	AlertDialogHeader,
	AlertDialogTitle,
} from "#/components/ui/alert-dialog.tsx";
import { Button } from "#/components/ui/button.tsx";
import {
	Dialog,
	DialogBody,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "#/components/ui/dialog.tsx";
import { Input } from "#/components/ui/input.tsx";
import { RadioGroup, RadioGroupItem } from "#/components/ui/radio-group.tsx";
import { Skeleton } from "#/components/ui/skeleton.tsx";
import {
	ApiError,
	api,
	type Grant,
	type GrantPrincipal,
} from "#/lib/api/index.ts";
import {
	type Choice,
	catalog,
	type Directory,
	expiresAt,
	expiryChoices,
	forTool,
	grantChoices,
	grantsAccess,
	kindLabel,
	principalKey,
	principalName,
	searchChoices,
} from "#/lib/grants.ts";

const appRoute = getRouteApi("/_app");

type Tool = (typeof catalog)[number];

type Loaded = { grants: Grant[]; names: Directory; choices: Choice[] };

// ponytail: people come from the first page of /v1/org/members, and agents from /v1/agents, which
// lists only the agents the admin may use; noted for the lead.
async function load(): Promise<Loaded> {
	const [grants, members, teams, agents] = await Promise.all([
		api.grants.list(),
		api.org.members(),
		api.org.teams(),
		api.agents(),
	]);
	const choices = grantChoices(members.items, teams.items, agents.items);
	const names: Directory = new Map(
		choices.map((c) => [principalKey(c.principal), c.name]),
	);
	return { grants: grants.items, names, choices };
}

const message = (e: unknown) =>
	e instanceof ApiError ? e.message : "Something went wrong. Try again.";

const day = (iso: string) => {
	const d = new Date(iso);
	return d.toLocaleDateString([], {
		day: "numeric",
		month: "short",
		...(d.getFullYear() === new Date().getFullYear()
			? {}
			: { year: "numeric" }),
	});
};

export function ToolsSection() {
	const { me } = appRoute.useLoaderData();
	const [data, setData] = useState<Loaded | null>(null);
	const [error, setError] = useState<unknown>(null);

	const refresh = useCallback(async () => {
		try {
			setData(await load());
			setError(null);
		} catch (e) {
			setError(e);
		}
	}, []);

	// The core decides every grant change; this only spares a non-admin controls that would fail.
	const access = grantsAccess(me, error);
	useEffect(() => {
		if (grantsAccess(me) === "manage") void refresh();
	}, [me, refresh]);

	return (
		<div className="flex flex-col gap-6">
			<SettingsHeader eyebrow="Connections" title="Tools">
				{access === "manage"
					? "Choose which people, teams and agents may use each tool, and for how long."
					: "Your organization's admins choose which tools agents may use, and for whom."}
			</SettingsHeader>
			{access === "not_admin" ? (
				<NotAdmin org={me.org.name} />
			) : error ? (
				<div className="rim flex flex-wrap items-center gap-3 rounded-window bg-raised px-5 py-4">
					<StatusMark mark="error" size={18} />
					<p
						role="alert"
						className="m-0 flex-1 text-[13px] text-soft-foreground"
					>
						{message(error)}
					</p>
					<Button variant="outline" onClick={refresh}>
						Try again
					</Button>
				</div>
			) : !data ? (
				<div className="flex flex-col gap-4" aria-busy="true">
					<Skeleton className="h-36 rounded-window" />
					<Skeleton className="h-36 rounded-window" />
				</div>
			) : (
				<>
					{catalog.map((tool) => (
						<ToolCard
							key={tool.id}
							tool={tool}
							grants={forTool(data.grants, tool.id)}
							data={data}
							onChanged={refresh}
						/>
					))}
					<p className="m-0 text-[12px] text-muted-foreground leading-normal">
						A grant lasts 90 days at most; allow it again when it ends. Removing
						one stops it at once.
					</p>
				</>
			)}
		</div>
	);
}

function NotAdmin({ org }: { org: string }) {
	return (
		<section className="rim flex items-start gap-3.5 rounded-window bg-raised p-[22px]">
			<span className="grid size-6 flex-none place-items-center text-muted-foreground">
				<Icon name="lock" size={20} strokeWidth={1.8} />
			</span>
			<div className="flex flex-col gap-1">
				<b className="font-semibold text-[14px]">
					Only admins choose which tools agents may use.
				</b>
				<span className="text-[13px] text-soft-foreground leading-normal">
					If your agent needs a tool it can't use yet, ask an admin of {org}.
				</span>
			</div>
		</section>
	);
}

function ToolCard({
	tool,
	grants,
	data,
	onChanged,
}: {
	tool: Tool;
	grants: Grant[];
	data: Loaded;
	onChanged: () => Promise<void>;
}) {
	// Opens with nobody chosen, or with whoever's grant ended.
	const [allowing, setAllowing] = useState<{
		prefill?: GrantPrincipal;
	} | null>(null);
	const [removing, setRemoving] = useState<Grant | null>(null);
	const [removeError, setRemoveError] = useState<string | null>(null);
	const [removeBusy, setRemoveBusy] = useState(false);
	const headingId = `${tool.id}-h`;

	const remove = async () => {
		if (!removing || removeBusy) return;
		setRemoveBusy(true);
		try {
			await api.grants.revoke(removing.id);
			setRemoving(null);
			await onChanged();
		} catch (e) {
			setRemoveError(message(e));
		} finally {
			setRemoveBusy(false);
		}
	};

	return (
		<section
			aria-labelledby={headingId}
			className="rim overflow-hidden rounded-window bg-raised"
		>
			<div className="flex flex-wrap items-center gap-3 p-4">
				<span className="grid size-6 flex-none place-items-center text-sea">
					<Icon
						name={tool.id === "web.read_page" ? "link" : "laptop"}
						size={20}
						strokeWidth={1.8}
					/>
				</span>
				<div className="flex min-w-[200px] flex-1 flex-col gap-0.5">
					<h4 id={headingId} className="m-0 font-semibold text-[14px]">
						{tool.title}
					</h4>
					<span className="text-[12px] text-muted-foreground leading-normal">
						{tool.about}
					</span>
				</div>
				<Button variant="outline" onClick={() => setAllowing({})}>
					Allow someone
				</Button>
			</div>
			{grants.length === 0 ? (
				<p className="m-0 border-border border-t px-4 py-3.5 text-[12.5px] text-muted-foreground">
					No one yet, so no agent can use it.
				</p>
			) : (
				<ul className="m-0 list-none p-0">
					{grants.map((g) => (
						<GrantRow
							key={g.id}
							grant={g}
							name={principalName(g.principal, data.names)}
							tool={tool}
							onAllowAgain={() => setAllowing({ prefill: g.principal })}
							onRemove={() => {
								setRemoveError(null);
								setRemoving(g);
							}}
						/>
					))}
				</ul>
			)}

			{allowing ? (
				<AllowDialog
					tool={tool}
					prefill={allowing.prefill}
					choices={data.choices}
					onClose={() => setAllowing(null)}
					onAllowed={async () => {
						setAllowing(null);
						await onChanged();
					}}
				/>
			) : null}

			<AlertDialog
				open={removing !== null}
				onOpenChange={(open) => {
					if (!open) setRemoving(null);
				}}
			>
				<AlertDialogContent>
					<AlertDialogHeader>
						<AlertDialogTitle>
							Remove{" "}
							{removing ? principalName(removing.principal, data.names) : ""}?
						</AlertDialogTitle>
						<AlertDialogDescription>
							Their agent stops being able to {tool.title.toLowerCase()} at
							once. You can allow them again later.
						</AlertDialogDescription>
					</AlertDialogHeader>
					{removeError ? (
						<p
							role="alert"
							className="m-0 text-[12.5px] text-danger-foreground"
						>
							{removeError}
						</p>
					) : null}
					<AlertDialogFooter>
						<AlertDialogCancel variant="outline">Keep</AlertDialogCancel>
						<AlertDialogAction
							variant="destructive"
							disabled={removeBusy}
							onClick={remove}
						>
							Remove
						</AlertDialogAction>
					</AlertDialogFooter>
				</AlertDialogContent>
			</AlertDialog>
		</section>
	);
}

function GrantRow({
	grant,
	name,
	tool,
	onAllowAgain,
	onRemove,
}: {
	grant: Grant;
	name: string;
	tool: Tool;
	onAllowAgain: () => void;
	onRemove: () => void;
}) {
	const expired = grant.state === "expired";
	return (
		<li className="flex flex-wrap items-center gap-x-3 gap-y-1 border-border border-t px-4 py-[11px]">
			<span
				aria-hidden="true"
				className={
					grant.principal.kind === "agent"
						? "grid size-7 flex-none place-items-center rounded-full bg-sky-soft font-bold text-[11px] text-sea"
						: "grid size-7 flex-none place-items-center rounded-full bg-active font-bold text-[11px]"
				}
			>
				{name[0]?.toUpperCase()}
			</span>
			<span className="flex min-w-[160px] flex-1 flex-col">
				<b
					className={
						expired
							? "font-semibold text-[13px] text-soft-foreground"
							: "font-semibold text-[13px]"
					}
				>
					{name}
				</b>
				<span className="truncate text-[11.5px] text-muted-foreground">
					{kindLabel[grant.principal.kind]} · allowed by {grant.granted_by.name}
					{grant.reason ? ` · “${grant.reason}”` : ""}
				</span>
			</span>
			<span className="text-[12px] text-muted-foreground tabular-nums">
				{expired
					? `Ended ${day(grant.expires_at)}`
					: `Until ${day(grant.expires_at)}`}
			</span>
			{expired ? (
				<Button variant="outline" size="sm" onClick={onAllowAgain}>
					Allow again
				</Button>
			) : (
				<Button
					variant="ghost"
					size="sm"
					aria-label={`Remove ${name} from ${tool.title}`}
					onClick={onRemove}
				>
					Remove
				</Button>
			)}
		</li>
	);
}

/** Who may use a tool, for how long, and whether without asking. */
function AllowDialog({
	tool,
	prefill,
	choices,
	onClose,
	onAllowed,
}: {
	tool: Tool;
	prefill?: GrantPrincipal;
	choices: Choice[];
	onClose: () => void;
	onAllowed: () => Promise<void>;
}) {
	const [query, setQuery] = useState("");
	const [picked, setPicked] = useState(prefill ? principalKey(prefill) : "");
	const [days, setDays] = useState("30");
	const [reason, setReason] = useState("");
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);

	const shown = searchChoices(choices, query, picked);
	const until = new Date(expiresAt(Number(days))).toLocaleDateString([], {
		weekday: "short",
		day: "numeric",
		month: "short",
	});

	const submit = async (e: FormEvent) => {
		e.preventDefault();
		const choice = choices.find((c) => principalKey(c.principal) === picked);
		if (!choice) {
			setError("Choose who to allow.");
			return;
		}
		setBusy(true);
		setError(null);
		try {
			await api.grants.create({
				principal: choice.principal,
				tool: tool.id,
				// ponytail: both v1 tools are external or plain reads, which the policy never runs
				// unattended, so a standing grant would change nothing; noted for the lead.
				standing: false,
				expires_at: expiresAt(Number(days)),
				...(reason.trim() ? { reason: reason.trim() } : {}),
			});
			await onAllowed();
		} catch (err) {
			setBusy(false);
			setError(
				err instanceof ApiError && err.code === "conflict"
					? `${choice.name} can already ${tool.title.toLowerCase()}.`
					: message(err),
			);
		}
	};

	return (
		<Dialog
			open
			onOpenChange={(open) => {
				if (!open) onClose();
			}}
		>
			<DialogContent className="w-[460px]">
				<DialogBody>
					<DialogHeader>
						<DialogTitle>
							Allow someone to {tool.title.toLowerCase()}
						</DialogTitle>
						<DialogDescription>
							Their agent may use it until the grant ends.{" "}
							{tool.id === "web.submit_form"
								? "It still asks first each time."
								: "Pages off your allowed sites still ask first."}
						</DialogDescription>
					</DialogHeader>

					<form
						id="allow-form"
						onSubmit={submit}
						className="flex flex-col gap-5"
					>
						<div className="flex flex-col gap-2">
							<label
								htmlFor="grant-who"
								className="font-semibold text-[12.5px]"
							>
								Who
							</label>
							<Input
								id="grant-who"
								placeholder="Search people, teams and agents"
								value={query}
								onChange={(e) => setQuery(e.target.value)}
								className="h-10"
							/>
							<RadioGroup
								aria-label="Who"
								value={picked}
								onValueChange={(v) => setPicked(String(v))}
								className="max-h-44 gap-0.5 overflow-y-auto rounded-[14px] p-1 shadow-[inset_0_0_0_1px_var(--border-quiet)]"
							>
								{shown.length === 0 ? (
									<p className="m-0 px-2.5 py-2 text-[12.5px] text-muted-foreground">
										No one matches.
									</p>
								) : null}
								{shown.map((c) => {
									const key = principalKey(c.principal);
									return (
										// biome-ignore lint/a11y/noLabelWithoutControl: the radio inside is the control
										<label
											key={key}
											className="flex cursor-pointer items-center gap-2.5 rounded-[10px] px-2.5 py-2 text-[13px] hover:bg-hover has-data-checked:bg-hover"
										>
											<RadioGroupItem value={key} />
											<span className="min-w-0 flex-1 truncate">{c.name}</span>
											<span className="text-[11.5px] text-muted-foreground">
												{kindLabel[c.principal.kind]}
											</span>
										</label>
									);
								})}
							</RadioGroup>
						</div>

						<div className="flex flex-col gap-2">
							<span id="grant-days" className="font-semibold text-[12.5px]">
								For how long
							</span>
							<RadioGroup
								aria-labelledby="grant-days"
								value={days}
								onValueChange={(v) => setDays(String(v))}
								className="rim grid grid-cols-4 gap-1 rounded-full bg-hover p-1"
							>
								{expiryChoices.map((d) => (
									// biome-ignore lint/a11y/noLabelWithoutControl: the radio inside is the control
									<label
										key={d}
										className="flex h-8 cursor-pointer items-center justify-center rounded-full font-medium text-[12.5px] text-soft-foreground has-focus-visible:ring-3 has-focus-visible:ring-ring/50 has-data-checked:bg-raised has-data-checked:font-semibold has-data-checked:text-foreground has-data-checked:shadow-[0_0_0_1px_var(--border-quiet)]"
									>
										<RadioGroupItem value={String(d)} className="sr-only" />
										{d === 1 ? "1 day" : `${d} days`}
									</label>
								))}
							</RadioGroup>
							<span className="text-[11.5px] text-muted-foreground">
								Until {until}.
							</span>
						</div>

						<div className="flex flex-col gap-2">
							<label
								htmlFor="grant-reason"
								className="font-semibold text-[12.5px]"
							>
								Reason{" "}
								<span className="font-normal text-muted-foreground">
									optional
								</span>
							</label>
							<Input
								id="grant-reason"
								placeholder="What it's for, so others know later"
								maxLength={500}
								value={reason}
								onChange={(e) => setReason(e.target.value)}
								className="h-10"
							/>
						</div>

						{error ? (
							<p
								role="alert"
								className="m-0 text-[12.5px] text-danger-foreground"
							>
								{error}
							</p>
						) : null}
					</form>

					<DialogFooter className="justify-end">
						<Button type="button" variant="outline" onClick={onClose}>
							Cancel
						</Button>
						<Button type="submit" form="allow-form" disabled={busy}>
							Allow
						</Button>
					</DialogFooter>
				</DialogBody>
			</DialogContent>
		</Dialog>
	);
}
