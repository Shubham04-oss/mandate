import { createFileRoute, notFound } from "@tanstack/react-router";
import { useState } from "react";
import { toast } from "sonner";
import { sceneUrl, Wordmark } from "#/components/auth/auth-layout.tsx";
import { Icon } from "#/components/icons/icon.tsx";
import {
	AlertDialog,
	AlertDialogAction,
	AlertDialogCancel,
	AlertDialogContent,
	AlertDialogDescription,
	AlertDialogFooter,
	AlertDialogHeader,
	AlertDialogMedia,
	AlertDialogTitle,
	AlertDialogTrigger,
} from "#/components/ui/alert-dialog.tsx";
import { Avatar, AvatarGroup, AvatarImage } from "#/components/ui/avatar.tsx";
import { Badge } from "#/components/ui/badge.tsx";
import { Button } from "#/components/ui/button.tsx";
import {
	Dialog,
	DialogBody,
	DialogClose,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogIcon,
	DialogScene,
	DialogTitle,
	DialogTrigger,
} from "#/components/ui/dialog.tsx";
import {
	Empty,
	EmptyContent,
	EmptyDescription,
	EmptyHeader,
	EmptyTitle,
} from "#/components/ui/empty.tsx";
import {
	Field,
	FieldContent,
	FieldDescription,
	FieldLabel,
} from "#/components/ui/field.tsx";
import {
	HoverCard,
	HoverCardContent,
	HoverCardTrigger,
} from "#/components/ui/hover-card.tsx";
import { InputGroup, InputGroupInput } from "#/components/ui/input-group.tsx";
import { Item, ItemContent, ItemMedia } from "#/components/ui/item.tsx";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "#/components/ui/select.tsx";
import { SceneToast } from "#/components/ui/sonner.tsx";
import { Switch } from "#/components/ui/switch.tsx";
import { brand } from "#/lib/brand.ts";
import { sample } from "#/lib/sample.ts";

// A review page for the pop-up patterns: dialogs with a scene, the plain confirm, toasts,
// the hover card and the empty state. Not linked from the app.
export const Route = createFileRoute("/preview/popups")({
	// A design preview with made-up people: never part of a real build.
	beforeLoad: () => {
		if (!import.meta.env.DEV) throw notFound();
	},
	head: () => ({ meta: [{ title: `Pop-ups · ${brand.name}` }] }),
	component: PopupsPreview,
});

const team = "Platform";

function PopupsPreview() {
	return (
		<div className="min-h-dvh bg-background px-14 py-12 text-foreground">
			<div className="flex flex-wrap items-start gap-10">
				<Cell title="Dialogs" note="a scene for moments, plain for warnings">
					<div className="flex flex-wrap gap-2">
						<WelcomeDialog />
						<InviteDialog />
						<RemoveDialog />
					</div>
				</Cell>

				<Cell title="Toasts" note="a strip of scene on the edge">
					<div className="flex flex-col gap-3">
						<AgentToast />
						<ApprovalToast />
						<SceneToast
							title="Link copied"
							description={`Anyone in ${sample.org.name} with the link can open it.`}
						/>
						<div className="flex gap-2">
							<Button
								variant="outline"
								onClick={() => toast.custom(() => <AgentToast />)}
							>
								Show a toast
							</Button>
							<Button
								variant="outline"
								onClick={() =>
									toast("Link copied", {
										description: `Anyone in ${sample.org.name} with the link can open it.`,
									})
								}
							>
								Show a plain toast
							</Button>
						</div>
					</div>
				</Cell>

				<Cell title="Hover card" note="a team, from the sidebar">
					<TeamHoverCard />
				</Cell>

				<Cell title="Empty state" note="ship lines on deep sea">
					<NothingWaiting />
				</Cell>
			</div>
		</div>
	);
}

function Cell({
	title,
	note,
	children,
}: {
	title: string;
	note: string;
	children: React.ReactNode;
}) {
	return (
		<section className="flex flex-col gap-2.5">
			<span className="font-semibold text-[11px] text-muted-foreground uppercase tracking-[0.04em]">
				<b className="mr-1.5 text-[12px] text-foreground normal-case tracking-normal">
					{title}
				</b>
				{note}
			</span>
			{children}
		</section>
	);
}

/** The one pop-up where the wordmark sits on the painting. */
function WelcomeDialog() {
	const steps = [
		{ label: "Connect Slack", done: true },
		{ label: "Invite your team", done: false },
		{ label: "Ask your agent its first question", done: false },
	];
	return (
		<Dialog>
			<DialogTrigger render={<Button variant="outline" />}>
				Welcome
			</DialogTrigger>
			<DialogContent scene className="w-[480px]">
				<DialogScene
					src={sceneUrl("crows-nest")}
					variant="hero"
					position="center 55%"
				>
					<Wordmark className="absolute top-[18px] left-[22px] z-10 text-white [text-shadow:0_1px_8px_rgba(0,0,0,0.25)]" />
				</DialogScene>
				<DialogBody className="[[data-slot=dialog-scene]+&]:-mt-[18px]">
					<DialogHeader>
						<DialogTitle>Welcome, {sample.me.name.split(" ")[0]}</DialogTitle>
						<DialogDescription>
							Your agent works only with what you grant it, and asks before it
							acts for you.
						</DialogDescription>
					</DialogHeader>
					<ol className="m-0 flex list-none flex-col gap-1 p-0">
						{steps.map((step, i) => (
							<Item
								key={step.label}
								render={<li />}
								size="xs"
								className={
									step.done
										? "h-[38px] rounded-md bg-canvas text-muted-foreground line-through"
										: "h-[38px] rounded-md bg-canvas text-soft-foreground"
								}
							>
								<ItemMedia>
									<span
										className={
											step.done
												? "grid size-5 place-items-center rounded-full bg-ok-solid text-white"
												: "grid size-5 place-items-center rounded-full bg-muted font-bold text-[10.5px] text-muted-foreground"
										}
									>
										{step.done ? (
											<Icon name="check" size={12} strokeWidth={3} />
										) : (
											i + 1
										)}
									</span>
								</ItemMedia>
								<ItemContent className="text-[12.5px]">
									{step.label}
								</ItemContent>
							</Item>
						))}
					</ol>
					<DialogFooter>
						<span className="flex-1 text-[12px] text-muted-foreground">
							1 of 3 done
						</span>
						<DialogClose render={<Button variant="outline" />}>
							Skip for now
						</DialogClose>
						<Button>Invite your team</Button>
					</DialogFooter>
				</DialogBody>
			</DialogContent>
		</Dialog>
	);
}

function InviteDialog() {
	const [byEmail, setByEmail] = useState(true);
	return (
		<Dialog>
			<DialogTrigger render={<Button variant="outline" />}>
				Invite
			</DialogTrigger>
			<DialogContent scene>
				<DialogScene src={sceneUrl("harbour")} />
				<DialogBody>
					<DialogIcon>
						<Icon name="user" />
					</DialogIcon>
					<DialogHeader>
						<DialogTitle>Invite people to {team}</DialogTitle>
						<DialogDescription>
							They join as members of the team. Their agents get nothing until
							someone grants it.
						</DialogDescription>
					</DialogHeader>
					<InputGroup className="h-auto min-h-10 flex-wrap gap-1.5 px-2 py-[5px]">
						<Badge
							variant="secondary"
							className="h-[26px] gap-1.5 rounded-full bg-muted pr-2 pl-[3px] text-[12px]"
						>
							<Avatar className="size-5">
								<AvatarImage src={sample.member.avatar} alt="" />
							</Avatar>
							Kenji Sato
						</Badge>
						<Badge
							variant="secondary"
							className="h-[26px] rounded-full bg-muted px-2.5 text-[12px]"
						>
							maria@northwind.de
						</Badge>
						<InputGroupInput
							aria-label="Add people or emails"
							placeholder="Add people or emails"
							className="h-7 w-auto min-w-36 px-1 text-[12.5px]"
						/>
					</InputGroup>
					<Field orientation="horizontal">
						<FieldLabel
							htmlFor="invite-role"
							className="flex-1 font-normal text-[12.5px] text-soft-foreground"
						>
							Their role
						</FieldLabel>
						<Select
							defaultValue="member"
							items={[
								{ value: "member", label: "Member" },
								{ value: "admin", label: "Admin" },
							]}
						>
							<SelectTrigger id="invite-role">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								<SelectItem value="member">Member</SelectItem>
								<SelectItem value="admin">Admin</SelectItem>
							</SelectContent>
						</Select>
					</Field>
					<Field orientation="horizontal">
						<FieldContent>
							<FieldLabel
								htmlFor="invite-email"
								className="font-normal text-[12.5px] text-soft-foreground"
							>
								Send the invitation by email
							</FieldLabel>
							<FieldDescription className="text-[11.5px]">
								Or copy the link and share it yourself.
							</FieldDescription>
						</FieldContent>
						<Switch
							id="invite-email"
							checked={byEmail}
							onCheckedChange={setByEmail}
						/>
					</Field>
					<DialogFooter>
						<Button variant="outline">
							<Icon name="link" size={14} />
							Copy link
						</Button>
						<span className="flex-1" />
						<Button>Send 2 invites</Button>
					</DialogFooter>
				</DialogBody>
			</DialogContent>
		</Dialog>
	);
}

/** Warnings and destructive confirms stay plain: no scene. */
function RemoveDialog() {
	return (
		<AlertDialog>
			<AlertDialogTrigger render={<Button variant="outline" />}>
				Remove
			</AlertDialogTrigger>
			<AlertDialogContent>
				<AlertDialogMedia className="text-destructive">
					<Icon name="trash" />
				</AlertDialogMedia>
				<AlertDialogHeader>
					<AlertDialogTitle>Remove Raya from {team}?</AlertDialogTitle>
					<AlertDialogDescription>
						Raya loses the team's access at once, and every grant she made to
						the team's agents ends with it. You can invite her again.
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel variant="outline">Cancel</AlertDialogCancel>
					<AlertDialogAction variant="destructive">Remove</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	);
}

function AgentToast() {
	return (
		<SceneToast
			scene={sceneUrl("bottles")}
			avatar={sample.agent.avatar}
			title="Your agent finished a task"
			description="4 sources read · 1 needs your approval"
			action={
				<Button variant="outline" size="sm">
					Open
				</Button>
			}
		/>
	);
}

function ApprovalToast() {
	return (
		<SceneToast
			scene={sceneUrl("harbour")}
			avatar={sample.org.inviter.avatar}
			title="Raya approved your request"
			description="Q3 revenue figures · the answer is ready"
			action={<Button size="sm">View</Button>}
		/>
	);
}

function TeamHoverCard() {
	return (
		<HoverCard>
			<HoverCardTrigger
				href="#platform"
				className="w-fit font-semibold text-[13px] text-foreground underline decoration-border underline-offset-4"
			>
				# {team}
			</HoverCardTrigger>
			<HoverCardContent side="right" align="start">
				<div
					aria-hidden="true"
					className="scene scene-fade-b scene-dim h-[72px]"
					style={{
						backgroundImage: `url(${sceneUrl("harbour")})`,
						backgroundPosition: "center 60%",
					}}
				/>
				<div className="-mt-[26px] relative flex flex-col gap-2.5 px-3.5 pb-3.5">
					<DialogIconStatic />
					<div className="flex flex-col gap-1">
						<b className="text-[14px] text-foreground">{team}</b>
						<p className="m-0 text-soft-foreground leading-normal">
							8 members · 4 agents
						</p>
					</div>
					<div className="flex items-center gap-2.5">
						<AvatarGroup>
							{[sample.me.avatar, ...sample.org.faces.slice(0, 3)].map(
								(src) => (
									<Avatar key={src} className="size-[26px]">
										<AvatarImage src={src} alt="" />
									</Avatar>
								),
							)}
						</AvatarGroup>
						<span className="flex-1" />
						<span className="inline-flex items-center gap-1.5 font-semibold text-[11.5px] text-ok-foreground">
							<i className="size-[7px] rounded-full bg-ok-solid" />2 agents
							working
						</span>
					</div>
				</div>
			</HoverCardContent>
		</HoverCard>
	);
}

function DialogIconStatic() {
	return (
		<span className="grid size-9 place-items-center">
			<Icon name="user" size={16} />
		</span>
	);
}

function NothingWaiting() {
	return (
		<Empty className="h-[300px] w-[420px] flex-none justify-end gap-2 bg-sea-deep pb-[34px]">
			<i
				aria-hidden="true"
				className="absolute inset-0 bg-[length:180px_auto] bg-[position:center_16px] bg-no-repeat opacity-[0.42]"
				style={{ backgroundImage: "url(/scenes/ship-lines.webp)" }}
			/>
			<EmptyHeader className="relative">
				<EmptyTitle className="text-white">Nothing waiting for you</EmptyTitle>
				<EmptyDescription className="max-w-[280px] text-[#c9d6e6]">
					When an agent needs your approval, the request lands here.
				</EmptyDescription>
			</EmptyHeader>
			<EmptyContent className="relative">
				<Button className="mt-1.5 bg-white text-sea-deep hover:bg-white/90">
					Ask your agent
				</Button>
			</EmptyContent>
		</Empty>
	);
}
