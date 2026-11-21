import { Link, type LinkProps } from "@tanstack/react-router";
import { cn } from "cn";
import type { FormEvent, ReactNode } from "react";
import { Icon, type IconName } from "#/components/icons/icon.tsx";
import { brand } from "#/lib/brand.ts";

export type Scene =
	| "bay"
	| "crows-nest"
	| "bottles"
	| "deck"
	| "gate"
	| "harbour";

export const sceneUrl = (scene: Scene) => `/scenes/${scene}.webp`;

export function Wordmark({ className }: { className?: string }) {
	return (
		<span
			className={cn(
				"font-bold font-wordmark text-[22px] leading-none tracking-[-0.055em]",
				className,
			)}
			role="img"
			aria-label={brand.name}
		>
			{brand.name}
			<i className="text-gold-deep not-italic">.</i>
		</span>
	);
}

type AuthLayoutProps = {
	scene: Scene;
	/** Optional overlay in the scene's top corner. */
	sceneCard?: ReactNode;
	children: ReactNode;
};

// The form on the app's surface with a painted scene beside it as one tile. Below the split
// breakpoint the scene becomes a band at the top that fades into the surface.
export function AuthLayout({ scene, sceneCard, children }: AuthLayoutProps) {
	return (
		<div className="flex min-h-dvh flex-col bg-background text-foreground split:grid split:h-dvh split:min-h-[640px] split:grid-cols-[minmax(0,1.15fr)_minmax(420px,1fr)]">
			<div
				aria-hidden="true"
				className="scene scene-soft relative m-3 mr-0 hidden overflow-hidden rounded-3xl split:block after:absolute after:inset-0 after:bg-linear-to-b after:from-50% after:from-[rgba(10,18,30,0)] after:to-[rgba(10,18,30,0.55)]"
				style={{ backgroundImage: `url(${sceneUrl(scene)})` }}
			>
				{sceneCard && (
					<div className="absolute top-7 right-7 z-10">{sceneCard}</div>
				)}
			</div>

			<div
				aria-hidden="true"
				className="scene scene-band relative h-[300px] flex-none bg-[center_60%] mask-b-from-55% split:hidden"
				style={{ backgroundImage: `url(${sceneUrl(scene)})` }}
			>
				<Wordmark className="absolute top-[54px] left-[22px] text-white [text-shadow:0_1px_8px_rgba(0,0,0,0.25)]" />
			</div>

			<div className="-mt-11 relative flex flex-1 flex-col px-[22px] pb-6 split:mt-0 split:px-10 split:py-7">
				<Wordmark className="hidden split:block" />
				<main className="flex w-full flex-1 flex-col split:m-auto split:max-w-[360px] split:flex-none">
					{children}
				</main>
				<footer className="mt-8 flex gap-4 text-[11.5px] text-muted-foreground split:mt-0">
					<span className="flex-1">
						© {new Date().getFullYear()} {brand.name}
					</span>
					<a
						href={brand.privacyUrl}
						className="text-inherit no-underline hover:text-foreground"
					>
						Privacy
					</a>
					<a
						href={brand.termsUrl}
						className="text-inherit no-underline hover:text-foreground"
					>
						Terms
					</a>
				</footer>
			</div>
		</div>
	);
}

const stack = "flex flex-col gap-3.5";

/** A screen that takes input. The handler gets the form's fields; the browser never posts it. */
export function AuthForm({
	label,
	onSubmit,
	children,
}: {
	label: string;
	onSubmit?: (data: FormData) => void;
	children: ReactNode;
}) {
	const submit = (e: FormEvent<HTMLFormElement>) => {
		e.preventDefault();
		onSubmit?.(new FormData(e.currentTarget));
	};
	return (
		<form aria-label={label} className={stack} onSubmit={submit} noValidate>
			{children}
		</form>
	);
}

/** What went wrong, said where the person is looking. */
export function FormError({ message }: { message?: string | null }) {
	if (!message) return null;
	return (
		<p
			role="alert"
			className="m-0 rounded-lg bg-destructive/12 px-3.5 py-2.5 text-[12.5px] text-danger-foreground leading-normal"
		>
			{message}
		</p>
	);
}

/** A quiet confirmation, such as "check your inbox". */
export function FormNote({ children }: { children?: ReactNode }) {
	if (!children) return null;
	return (
		<p
			aria-live="polite"
			className="m-0 rounded-lg bg-muted px-3.5 py-2.5 text-[12.5px] text-soft-foreground leading-normal"
		>
			{children}
		</p>
	);
}

/** A screen that reports where the person stands. */
export function AuthStatus({
	label,
	alert,
	children,
}: {
	label: string;
	alert?: boolean;
	children: ReactNode;
}) {
	return (
		<section
			role={alert ? "alert" : "status"}
			aria-label={label}
			className={stack}
		>
			{children}
		</section>
	);
}

export function Title({ children }: { children: ReactNode }) {
	return (
		<h1 className="m-0 font-bold text-[26px] tracking-[-0.02em] split:text-[28px]">
			{children}
		</h1>
	);
}

export function Lead({ children }: { children: ReactNode }) {
	return (
		<p className="-mt-1.5 mb-1.5 text-[13.5px] text-muted-foreground leading-normal">
			{children}
		</p>
	);
}

/** The quiet line under a form: "New to oryxa? Create an account". */
export function Aside({ children }: { children: ReactNode }) {
	return (
		<p className="m-0 text-center text-[12.5px] text-muted-foreground">
			{children}
		</p>
	);
}

export function TextLink({
	tone = "strong",
	className,
	...props
}: LinkProps & {
	tone?: "strong" | "muted";
	className?: string;
	children: ReactNode;
}) {
	return (
		<Link
			className={cn(
				"no-underline hover:underline",
				tone === "strong"
					? "font-semibold text-foreground"
					: "font-medium text-muted-foreground",
				className,
			)}
			{...props}
		/>
	);
}

const statusTones = {
	neutral: "bg-raised text-foreground shadow-tile",
	ok: "bg-ok text-ok-foreground",
	danger: "bg-destructive/15 text-danger-foreground",
};

/** The tile at the top of a status screen ("Check your inbox", "Email verified"). */
export function StatusIcon({
	icon,
	tone = "neutral",
}: {
	icon: IconName;
	tone?: keyof typeof statusTones;
}) {
	return (
		<span
			className={cn(
				"grid size-14 place-items-center rounded-2xl",
				statusTones[tone],
			)}
		>
			<Icon name={icon} size={24} strokeWidth={tone === "ok" ? 2.2 : 1.7} />
		</span>
	);
}

/** The frosted note that sits on the sign-in scene. */
export function SceneCard({
	avatar,
	title,
	line,
}: {
	avatar: string;
	title: string;
	line: string;
}) {
	return (
		<div className="flex items-center gap-2.5 rounded-section bg-[rgba(12,20,34,0.34)] py-2.5 pr-3.5 pl-2.5 text-[12px] text-white shadow-[inset_0_0_0_1px_rgba(255,255,255,0.22)] backdrop-blur-[14px] backdrop-saturate-[1.2]">
			<img
				src={avatar}
				alt=""
				width={30}
				height={30}
				className="rounded-full"
			/>
			<div>
				<b className="block text-[12.5px]">{title}</b>
				<span className="text-white/80">{line}</span>
			</div>
		</div>
	);
}
