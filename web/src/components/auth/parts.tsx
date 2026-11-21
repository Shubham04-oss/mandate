import { cn } from "cn";
import { type ComponentProps, type ReactNode, useState } from "react";
import { TextLink } from "#/components/auth/auth-layout.tsx";
import { GoogleMark, Icon } from "#/components/icons/icon.tsx";
import { Avatar, AvatarGroup, AvatarImage } from "#/components/ui/avatar.tsx";
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
import { normalizeOtp, OTP_LENGTH } from "#/lib/otp.ts";
import { checkPassword } from "#/lib/password.ts";

export function GoogleButton({
	children,
	lastUsed,
}: {
	children: ReactNode;
	lastUsed?: boolean;
}) {
	return (
		<Button variant="secondary" size="lg">
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
	return (
		<InputGroup>
			<InputGroupInput type={shown ? "text" : "password"} {...props} />
			<InputGroupAddon align="inline-end">
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
	return (
		<InputOTP
			id={id}
			name="code"
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

type Invite = {
	name: string;
	initial: string;
	summary: string;
	faces: string[];
	facesLine: string;
	inviter: { name: string; avatar: string };
};

export function InviteCard({ org }: { org: Invite }) {
	return (
		<Card className="gap-3 p-4">
			<div className="flex items-center gap-3">
				<OrgMark initial={org.initial} brand="var(--sea-deep)" />
				<div>
					<b className="block text-[15px]">{org.name}</b>
					<small className="text-[12px] text-muted-foreground">
						{org.summary}
					</small>
				</div>
			</div>
			<div className="flex items-center gap-2.5 text-[12px] text-muted-foreground">
				<AvatarGroup>
					{org.faces.map((src) => (
						<Avatar key={src} className="size-[22px]">
							<AvatarImage src={src} alt="" />
						</Avatar>
					))}
				</AvatarGroup>
				<span>{org.facesLine}</span>
			</div>
			<div className="flex items-center gap-2.5 border-t pt-3 text-[12.5px] text-soft-foreground leading-[1.45]">
				<Avatar className="size-7">
					<AvatarImage src={org.inviter.avatar} alt="" />
				</Avatar>
				<span>
					<b className="text-foreground">{org.inviter.name}</b> invited you to
					join as a member.
				</span>
			</div>
		</Card>
	);
}

/** One organization to pick, on the screen shown when a person belongs to several. */
export function OrgChoice({
	name,
	initial,
	summary,
}: {
	name: string;
	initial: string;
	summary: string;
}) {
	return (
		<Item
			variant="raised"
			size="sm"
			className="p-3"
			render={<button type="submit" name="organization" value={name} />}
		>
			<ItemMedia>
				<OrgMark initial={initial} size={36} />
			</ItemMedia>
			<ItemContent className="gap-0.5">
				<ItemTitle>{name}</ItemTitle>
				<ItemDescription>{summary}</ItemDescription>
			</ItemContent>
			<ItemActions>
				<Icon name="chevron-right" className="text-muted-foreground" />
			</ItemActions>
		</Item>
	);
}

/** "Signed in as …  Use another account" */
export function SignedInAs({
	email,
	avatar,
}: {
	email: string;
	avatar: string;
}) {
	return (
		<div className="flex items-center gap-2 text-[12px] text-muted-foreground">
			<Avatar className="size-5">
				<AvatarImage src={avatar} alt="" />
			</Avatar>
			Signed in as {email}
			<TextLink to="/sign-in" className="ml-auto">
				Use another account
			</TextLink>
		</div>
	);
}
