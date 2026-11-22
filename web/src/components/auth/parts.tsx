import { cn } from "cn";
import {
	type ComponentProps,
	type KeyboardEvent,
	type ReactNode,
	useRef,
	useState,
} from "react";
import { TextLink } from "#/components/auth/auth-layout.tsx";
import { GoogleMark, Icon } from "#/components/icons/icon.tsx";
import {
	Avatar,
	AvatarFallback,
	AvatarImage,
} from "#/components/ui/avatar.tsx";
import { Badge } from "#/components/ui/badge.tsx";
import { Button } from "#/components/ui/button.tsx";
import { Card } from "#/components/ui/card.tsx";
import {
	InputGroup,
	InputGroupAddon,
	InputGroupButton,
	InputGroupInput,
} from "#/components/ui/input-group.tsx";
import {
	InputOTP,
	InputOTPGroup,
	InputOTPSlot,
} from "#/components/ui/input-otp.tsx";
import {
	Item,
	ItemActions,
	ItemContent,
	ItemDescription,
	ItemMedia,
	ItemTitle,
} from "#/components/ui/item.tsx";
import type { Invitation, Org, Person } from "#/lib/api/index.ts";
import { normalizeOtp, OTP_LENGTH } from "#/lib/otp.ts";
import { checkPassword } from "#/lib/password.ts";

export function GoogleButton({
	children,
	lastUsed,
	disabled,
	title,
}: {
	children: ReactNode;
	lastUsed?: boolean;
	disabled?: boolean;
	title?: string;
}) {
	return (
		<Button variant="secondary" size="lg" disabled={disabled} title={title}>
			<GoogleMark />
			{children}
			{lastUsed && (
				<Badge
					variant="success"
					className="-translate-y-1/2 absolute top-1/2 right-2.5"
				>
					Last used
				</Badge>
			)}
		</Button>
	);
}

export function PasswordInput(props: Omit<ComponentProps<"input">, "type">) {
	const [shown, setShown] = useState(false);
	// A password typed in capitals fails without saying why; say it while they type.
	const [caps, setCaps] = useState(false);
	const readCaps = (e: KeyboardEvent<HTMLInputElement>) =>
		setCaps(e.getModifierState("CapsLock"));
	return (
		<InputGroup>
			<InputGroupInput
				type={shown ? "text" : "password"}
				{...props}
				onKeyDown={readCaps}
				onKeyUp={readCaps}
				onBlur={() => setCaps(false)}
			/>
			<InputGroupAddon align="inline-end">
				{caps ? (
					<span
						aria-live="polite"
						className="whitespace-nowrap font-medium text-[11px] text-soft-foreground"
					>
						Caps Lock is on
					</span>
				) : null}
				<InputGroupButton
					size="icon-sm"
					aria-label={shown ? "Hide password" : "Show password"}
					aria-pressed={shown}
					onClick={() => setShown((s) => !s)}
				>
					<Icon name={shown ? "eye-off" : "eye"} strokeWidth={1.8} />
				</InputGroupButton>
			</InputGroupAddon>
		</InputGroup>
	);
}

/** Four segments that fill as the password meets more of the rules. */
export function StrengthMeter({ password }: { password: string }) {
	const { score, level } = checkPassword(password);
	return (
		// biome-ignore lint/a11y/useSemanticElements: four segments can't be drawn with a styled <meter>
		<div
			className="grid grid-cols-4 gap-1"
			role="meter"
			aria-label="Password strength"
			aria-valuemin={0}
			aria-valuemax={4}
			aria-valuenow={score}
			aria-valuetext={level}
		>
			{[1, 2, 3, 4].map((step) => (
				<i
					key={step}
					className={cn(
						"h-1 rounded-[2px]",
						step <= score ? "bg-ok-solid" : "bg-muted",
					)}
				/>
			))}
		</div>
	);
}

export function PasswordRules({ password }: { password: string }) {
	const { results } = checkPassword(password);
	return (
		<ul className="m-0 grid list-none grid-cols-2 gap-x-3 gap-y-1.5 p-0 text-[12px]">
			{results.map((rule) => (
				<li
					key={rule.key}
					className={cn(
						"inline-flex items-center gap-1.5",
						rule.ok ? "text-soft-foreground" : "text-muted-foreground",
					)}
				>
					<Icon
						name={rule.ok ? "check" : "dot"}
						size={13}
						strokeWidth={rule.ok ? 2.4 : 2}
						className={rule.ok ? "text-ok-solid" : undefined}
					/>
					{rule.label}
					<span className="sr-only">{rule.ok ? "(met)" : "(not met)"}</span>
				</li>
			))}
		</ul>
	);
}

/** Six wells for a one-time code; paste and the phone's code autofill both work. */
export function CodeInput({
	id,
	label,
	autoFocus,
}: {
	id: string;
	label: string;
	autoFocus?: boolean;
}) {
	const input = useRef<HTMLInputElement>(null);
	return (
		<InputOTP
			ref={input}
			id={id}
			name="code"
			// the sixth digit sends the form; nothing else is left to fill in
			onComplete={() => input.current?.form?.requestSubmit()}
			aria-label={label}
			maxLength={OTP_LENGTH}
			inputMode="numeric"
			pattern="^\d*$"
			pasteTransformer={normalizeOtp}
			autoFocus={autoFocus}
			containerClassName="w-full"
		>
			<InputOTPGroup className="w-full">
				{Array.from({ length: OTP_LENGTH }, (_, i) => (
					// biome-ignore lint/suspicious/noArrayIndexKey: slots are positions, not items
					<InputOTPSlot key={i} index={i} />
				))}
			</InputOTPGroup>
		</InputOTP>
	);
}

/** A square initial standing in for an organization's logo. */
export function OrgMark({
	initial,
	size = 44,
	brand,
}: {
	initial: string;
	size?: number;
	/** The organization's colour, when it has one; otherwise a neutral tile. */
	brand?: string;
}) {
	return (
		<span
			className={cn(
				"grid shrink-0 place-items-center rounded-[13px] font-bold",
				brand ? "text-white" : "bg-selected text-foreground",
			)}
			style={{
				width: size,
				height: size,
				fontSize: Math.round(size * 0.41),
				backgroundColor: brand,
			}}
			aria-hidden="true"
		>
			{initial}
		</span>
	);
}

export function InviteCard({ invitation }: { invitation: Invitation }) {
	const { org, inviter } = invitation;
	return (
		<Card className="gap-3 p-4">
			<div className="flex items-center gap-3">
				<OrgMark initial={org.name[0] ?? "?"} brand="var(--sea-deep)" />
				<div>
					<b className="block text-[15px]">{org.name}</b>
					<small className="text-[12px] text-muted-foreground">
						{orgSummary(org)}
					</small>
				</div>
			</div>
			<div className="flex items-center gap-2.5 border-t pt-3 text-[12.5px] text-soft-foreground leading-[1.45]">
				<PersonAvatar person={inviter} className="size-7" />
				<span>
					<b className="text-foreground">{inviter.name}</b> invited{" "}
					{invitation.email} to join as a member.
				</span>
			</div>
		</Card>
	);
}

export const orgSummary = (org: Org) =>
	`${org.summary.people} ${org.summary.people === 1 ? "person" : "people"} · ${org.summary.agents} ${
		org.summary.agents === 1 ? "agent" : "agents"
	}`;

/** A person's picture, or their initial when they have none. */
export function PersonAvatar({
	person,
	className,
}: {
	person: Person;
	className?: string;
}) {
	return (
		<Avatar className={className}>
			{person.avatar_url ? (
				<AvatarImage src={person.avatar_url} alt="" />
			) : null}
			<AvatarFallback className="text-[11px]">
				{person.name[0] ?? "?"}
			</AvatarFallback>
		</Avatar>
	);
}

/** One organization to pick, on the screen shown when a person belongs to several. */
export function OrgChoice({
	org,
	disabled,
	onSelect,
}: {
	org: Org;
	disabled?: boolean;
	onSelect: () => void;
}) {
	return (
		<Item
			variant="raised"
			size="sm"
			className="p-3"
			render={<button type="button" disabled={disabled} onClick={onSelect} />}
		>
			<ItemMedia>
				<OrgMark initial={org.name[0] ?? "?"} size={36} />
			</ItemMedia>
			<ItemContent className="gap-0.5">
				<ItemTitle>{org.name}</ItemTitle>
				<ItemDescription>{orgSummary(org)}</ItemDescription>
			</ItemContent>
			<ItemActions>
				<Icon name="chevron-right" className="text-muted-foreground" />
			</ItemActions>
		</Item>
	);
}

/** "Signing in as …  Use another account" */
export function SignedInAs({
	email,
	label = "Signing in as",
}: {
	email: string;
	label?: string;
}) {
	return (
		<div className="flex items-center gap-2 text-[12px] text-muted-foreground">
			{label} {email}
			<TextLink to="/sign-in" className="ml-auto">
				Use another account
			</TextLink>
		</div>
	);
}
