import { cn } from "cn";
import { useState } from "react";
import { Icon } from "#/components/icons/icon.tsx";
import { AppLogo } from "#/components/settings/app-logo.tsx";
import { ConnectKeyDialog } from "#/components/settings/connect-key-dialog.tsx";
import { Badge } from "#/components/ui/badge.tsx";
import { Button } from "#/components/ui/button.tsx";
import {
	Collapsible,
	CollapsibleContent,
	CollapsibleTrigger,
} from "#/components/ui/collapsible.tsx";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "#/components/ui/dropdown-menu.tsx";
import {
	InputGroup,
	InputGroupAddon,
	InputGroupInput,
} from "#/components/ui/input-group.tsx";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "#/components/ui/select.tsx";
import { sample } from "#/lib/sample.ts";
import {
	type CatalogTool,
	type ConnectedTool,
	searchCatalog,
	summarizeConnections,
	type ToolStatus,
} from "#/lib/tools.ts";

// ponytail: every control here changes local state only. Connecting, disconnecting, events and who
// can use a tool all go through the core, which holds the credentials in the token vault.

// The colour lives in the dot, the text stays neutral. Orange, and its warm tint, only where the
// person must act.
const statusPill: Record<
	Exclude<ToolStatus, "connected">,
	{ label: string; dot: string; fill: string }
> = {
	connecting: { label: "Connecting…", dot: "bg-sea", fill: "bg-hover" },
	expired: { label: "Expired", dot: "bg-gold-deep", fill: "bg-gold-soft" },
	failed: { label: "Failed", dot: "bg-destructive", fill: "bg-hover" },
};

export function ToolsSection() {
	const [connected, setConnected] = useState(sample.tools.connected);
	const [query, setQuery] = useState("");
	const connectedIds = new Set(connected.map((t) => t.id));
	const groups = searchCatalog(
		sample.tools.catalog.map((g) => ({
			...g,
			tools: g.tools.filter((t) => !connectedIds.has(t.id)),
		})),
		query,
	);

	return (
		<div className="flex flex-col gap-8">
			<header className="flex flex-col gap-3">
				<span className="eyebrow">Connections</span>
				<h2 className="m-0 font-semibold text-[22px] tracking-[-0.016em]">
					Tools
				</h2>
				<p className="m-0 max-w-[560px] text-[13.5px] text-soft-foreground leading-normal">
					Agents use a tool only with the access its person already has, and
					only for the people you allow here.
				</p>
			</header>

			<section aria-labelledby="connected-h" className="flex flex-col gap-2.5">
				<div className="flex flex-wrap items-center gap-2">
					<h3
						id="connected-h"
						className="m-0 font-semibold text-[16px] tracking-[-0.01em]"
					>
						Connected
					</h3>
					<span className="ml-auto rounded-full px-2.5 py-1 text-[11px] text-soft-foreground shadow-[inset_0_0_0_1px_var(--border-quiet)]">
						{summarizeConnections(connected)}
					</span>
				</div>
				<ul className="m-0 flex list-none flex-col rounded-card bg-raised p-0 shadow-card">
					{connected.map((tool) => (
						<ConnectedRow
							key={tool.id}
							tool={tool}
							onChange={(next) =>
								setConnected((all) =>
									all.map((t) => (t.id === next.id ? next : t)),
								)
							}
							onRemove={() =>
								setConnected((all) => all.filter((t) => t.id !== tool.id))
							}
						/>
					))}
				</ul>
			</section>

			<section aria-labelledby="add-h" className="flex flex-col gap-5">
				<div className="flex flex-wrap items-end justify-between gap-3">
					<div className="flex flex-col gap-1">
						<h3
							id="add-h"
							className="m-0 font-semibold text-[16px] tracking-[-0.01em]"
						>
							Add a tool
						</h3>
						<p className="m-0 text-[12.5px] text-muted-foreground">
							Browse by what your team needs, or search.
						</p>
					</div>
					<InputGroup className="h-9 w-full rounded-lg sm:w-64">
						<InputGroupAddon align="inline-start">
							<Icon name="search" size={15} className="text-muted-foreground" />
						</InputGroupAddon>
						<InputGroupInput
							aria-label="Search tools"
							placeholder="Search tools"
							value={query}
							onChange={(e) => setQuery(e.target.value)}
							className="pl-1 text-[12.5px]"
						/>
					</InputGroup>
				</div>

				{groups.length === 0 ? (
					<p className="m-0 py-6 text-center text-[13px] text-muted-foreground">
						No tools match. Try a different search.
					</p>
				) : (
					groups.map((group) => (
						<section
							key={group.title}
							aria-label={group.title}
							className="flex flex-col gap-2.5"
						>
							<div className="flex items-baseline gap-2">
								<h4 className="m-0 font-semibold text-[14px]">{group.title}</h4>
								<span className="text-[12px] text-muted-foreground max-sm:hidden">
									{group.description}
								</span>
								<span className="ml-auto text-[11px] text-muted-foreground tabular-nums">
									{String(group.tools.length).padStart(2, "0")}
								</span>
							</div>
							<ul className="m-0 grid list-none grid-cols-1 items-start gap-2 p-0 sm:grid-cols-2">
								{group.tools.map((tool) => (
									<CatalogCard key={tool.id} tool={tool} />
								))}
							</ul>
						</section>
					))
				)}
			</section>
		</div>
	);
}

function ConnectedRow({
	tool,
	onChange,
	onRemove,
}: {
	tool: ConnectedTool;
	onChange: (tool: ConnectedTool) => void;
	onRemove: () => void;
}) {
	const [open, setOpen] = useState(false);
	const [confirming, setConfirming] = useState(false);
	const access =
		sample.tools.accessOptions.find((o) => o.value === tool.access)?.label ??
		"";
	const pill = tool.status === "connected" ? null : statusPill[tool.status];

	return (
		<li className="border-border border-t first:border-t-0">
			<Collapsible open={open} onOpenChange={setOpen}>
				<div className="flex items-center gap-3 px-3 py-2.5">
					<CollapsibleTrigger className="flex min-w-0 flex-1 cursor-pointer items-center gap-3 rounded-lg border-0 bg-transparent p-0 text-left text-foreground">
						<AppLogo logo={tool.logo} name={tool.name} />
						<span className="flex min-w-0 flex-col gap-0.5">
							<span className="flex items-center gap-2 font-semibold text-[13px]">
								{tool.name}
								{pill ? (
									<Badge
										className={cn(
											"h-5 gap-1.5 pr-2 pl-1.5 font-medium text-[11px] text-soft-foreground",
											pill.fill,
										)}
									>
										<i className={cn("status-dot", pill.dot)} />
										{pill.label}
									</Badge>
								) : null}
							</span>
							<span className="truncate text-[12px] text-muted-foreground">
								{access} · {tool.detail}
							</span>
						</span>
					</CollapsibleTrigger>

					<AccessSelect
						tool={tool}
						onChange={onChange}
						className="max-sm:hidden"
					/>

					<DropdownMenu>
						<DropdownMenuTrigger
							render={
								<Button
									variant="ghost"
									size="icon-sm"
									aria-label={`More for ${tool.name}`}
								/>
							}
						>
							<span
								aria-hidden="true"
								className="font-bold text-[15px] leading-none tracking-[1px]"
							>
								···
							</span>
						</DropdownMenuTrigger>
						<DropdownMenuContent align="end" className="w-48">
							<DropdownMenuItem onClick={() => setOpen(true)}>
								Events and details
							</DropdownMenuItem>
							{tool.status !== "connected" ? (
								<DropdownMenuItem>Reconnect ↗</DropdownMenuItem>
							) : null}
							<DropdownMenuItem
								variant="destructive"
								onClick={() => {
									setOpen(true);
									setConfirming(true);
								}}
							>
								Disconnect
							</DropdownMenuItem>
						</DropdownMenuContent>
					</DropdownMenu>
				</div>

				<CollapsibleContent>
					<div className="flex flex-col gap-3 pr-3 pb-3.5 pl-[58px]">
						<div className="flex items-center gap-2 text-[12px] text-muted-foreground sm:hidden">
							Who can use it
							<AccessSelect tool={tool} onChange={onChange} />
						</div>

						{tool.status === "connected" ? (
							<div className="flex flex-col rounded-lg bg-canvas shadow-ring">
								<div className="flex items-center gap-2 px-3 py-2.5">
									<span className="flex flex-col gap-0.5">
										<b className="font-semibold text-[12.5px]">
											Events that wake your agent
										</b>
										<span className="text-[11.5px] text-muted-foreground">
											When one happens in {tool.name}, your agent can act on it.
										</span>
									</span>
									<Button variant="outline" size="xs" className="ml-auto">
										+ Add event
									</Button>
								</div>
								{tool.events.length === 0 ? (
									<p className="m-0 border-border border-t px-3 py-3 text-[12px] text-muted-foreground">
										No events yet.
									</p>
								) : (
									tool.events.map((event) => (
										<div
											key={event.id}
											className="flex items-center gap-2.5 border-border border-t px-3 py-2.5"
										>
											<i
												aria-hidden="true"
												className={cn(
													"size-[7px] shrink-0 rounded-full",
													event.on
														? "bg-ok-solid shadow-[0_0_0_3px_rgb(127_207_152/0.15)]"
														: "bg-border-strong",
												)}
											/>
											<span className="flex min-w-0 flex-1 flex-col gap-0.5">
												<b className="font-semibold text-[12px]">
													{event.title}
												</b>
												<span className="text-[11px] text-muted-foreground">
													{event.detail}
												</span>
											</span>
											<button
												type="button"
												className="cursor-pointer border-0 bg-transparent p-0 text-[11.5px] text-muted-foreground hover:text-foreground"
												onClick={() =>
													onChange({
														...tool,
														events: tool.events.map((e) =>
															e.id === event.id ? { ...e, on: !e.on } : e,
														),
													})
												}
											>
												{event.on ? "Stop" : "Turn on"}
											</button>
										</div>
									))
								)}
							</div>
						) : (
							<p className="m-0 text-[12px] text-muted-foreground">
								{tool.status === "connecting"
									? `Finish signing in to ${tool.name} in the tab that opened.`
									: `${tool.name}'s sign-in has run out. Reconnect to keep using it.`}
							</p>
						)}

						{confirming ? (
							<div className="flex flex-wrap items-center gap-2 rounded-lg bg-destructive/10 px-3 py-2.5 shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--danger)_35%,transparent)]">
								<span className="flex-1 text-[12px] text-soft-foreground">
									Disconnect {tool.name}? Agents lose it at once, and every
									grant that uses it stops.
								</span>
								<Button
									variant="outline"
									size="xs"
									onClick={() => setConfirming(false)}
								>
									Keep connected
								</Button>
								<Button variant="destructive" size="xs" onClick={onRemove}>
									Yes, disconnect
								</Button>
							</div>
						) : (
							<div className="flex gap-2">
								{tool.status !== "connected" ? (
									<Button size="xs">Reconnect ↗</Button>
								) : null}
								<Button
									variant="outline"
									size="xs"
									onClick={() => setConfirming(true)}
								>
									Disconnect
								</Button>
							</div>
						)}
					</div>
				</CollapsibleContent>
			</Collapsible>
		</li>
	);
}

/** Who may use the tool through their agents. */
function AccessSelect({
	tool,
	onChange,
	className,
}: {
	tool: ConnectedTool;
	onChange: (tool: ConnectedTool) => void;
	className?: string;
}) {
	return (
		<Select
			value={tool.access}
			onValueChange={(value) => value && onChange({ ...tool, access: value })}
			items={sample.tools.accessOptions}
		>
			<SelectTrigger
				aria-label={`Who can use ${tool.name}`}
				className={className}
			>
				<SelectValue />
			</SelectTrigger>
			<SelectContent>
				{sample.tools.accessOptions.map((o) => (
					<SelectItem key={o.value} value={o.value}>
						{o.label}
					</SelectItem>
				))}
			</SelectContent>
		</Select>
	);
}

function CatalogCard({ tool }: { tool: CatalogTool }) {
	const [keyOpen, setKeyOpen] = useState(false);
	return (
		<li>
			<Collapsible className="rounded-xl bg-raised shadow-ring transition-shadow duration-150 hover:shadow-[0_0_0_1px_var(--border-strong)] data-open:shadow-[0_0_0_1px_var(--border-strong)]">
				<CollapsibleTrigger className="group flex w-full cursor-pointer items-center gap-2.5 rounded-xl border-0 bg-transparent p-2 text-left text-foreground">
					<AppLogo logo={tool.logo} name={tool.name} size={38} />
					<span className="flex-1 font-semibold text-[13px]">{tool.name}</span>
					<Icon
						name="chevron-right"
						size={15}
						className="mr-1 rotate-90 text-muted-foreground transition-transform duration-150 group-data-panel-open:-rotate-90"
					/>
				</CollapsibleTrigger>
				<CollapsibleContent>
					<div className="flex flex-col items-start gap-2.5 pr-3 pb-3 pl-[56px]">
						<p className="m-0 text-[12px] text-muted-foreground leading-normal">
							{tool.description}
						</p>
						{tool.keyFields ? (
							<Button
								variant="outline"
								size="xs"
								onClick={() => setKeyOpen(true)}
							>
								Connect with a key
							</Button>
						) : (
							<Button variant="outline" size="xs">
								Connect ↗
							</Button>
						)}
					</div>
				</CollapsibleContent>
			</Collapsible>
			{tool.keyFields ? (
				<ConnectKeyDialog
					tool={tool}
					open={keyOpen}
					onOpenChange={setKeyOpen}
				/>
			) : null}
		</li>
	);
}
