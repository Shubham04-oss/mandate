import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import {
	Aside,
	AuthForm,
	AuthLayout,
	FormError,
	Lead,
	StatusIcon,
	TextLink,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { CodeInput } from "#/components/auth/parts.tsx";
import { StartAgain } from "#/components/auth/start-again.tsx";
import { field, useSignIn } from "#/components/auth/use-sign-in.ts";
import { Badge } from "#/components/ui/badge.tsx";
import { Button } from "#/components/ui/button.tsx";
import { Skeleton } from "#/components/ui/skeleton.tsx";
import { api, type MfaSetup as Setup } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { readFlow, rememberSetup } from "#/lib/sign-in-flow.ts";

// `next: mfa_setup`: the organization requires two-step and this person has none yet.
export const Route = createFileRoute("/mfa_/setup")({
	head: () => ({
		meta: [{ title: `Set up two-step sign-in · ${brand.name}` }],
	}),
	ssr: false,
	component: MfaSetup,
});

/** The QR arrives as SVG text; an <img> shows it without letting it run anything. */
const qrSrc = (svg: string) =>
	`data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;

function MfaSetup() {
	const [flow] = useState(readFlow);
	const pending = flow.pending;
	// The core spends the pending step on setup, so after a reload only the saved answer works.
	const [setup, setSetup] = useState<Setup | null>(flow.setup ?? null);
	const { busy, error, run, act } = useSignIn();

	const fetchSetup = () => {
		if (!pending) return;
		act(
			() => api.auth.mfaSetup(pending),
			(answer) => {
				rememberSetup(answer);
				setSetup(answer);
			},
		);
	};

	// biome-ignore lint/correctness/useExhaustiveDependencies: fetch the factor once per pending sign-in
	useEffect(() => {
		if (!setup) fetchSetup();
	}, [pending]);

	if (!pending) return <StartAgain />;

	return (
		<AuthLayout scene="crows-nest">
			<AuthForm
				label="Set up two-step sign-in"
				onSubmit={(data) => {
					if (!setup) return;
					run(() =>
						api.auth.mfaEnable({
							pending,
							challenge: setup.challenge,
							code: field(data, "code"),
						}),
					);
				}}
			>
				<StatusIcon icon="shield-check" />
				<Title>Set up two-step sign-in</Title>
				<Lead>
					Your organization asks for a code from an authenticator app. Scan this
					with one, then enter the code it shows.
				</Lead>
				<div className="flex items-center gap-4 rounded-section bg-raised p-3 shadow-raised">
					{setup ? (
						<img
							src={qrSrc(setup.qr_svg)}
							alt="QR code for your authenticator app"
							width={112}
							height={112}
							className="shrink-0 rounded-item bg-white p-2"
						/>
					) : (
						<Skeleton className="size-28 shrink-0 rounded-item" />
					)}
					<div className="flex flex-col gap-2 text-[12px] text-muted-foreground leading-normal">
						<span>Can't scan it? Enter this key instead.</span>
						{setup ? (
							<Badge
								variant="secondary"
								className="w-fit font-mono text-[12px] tracking-wide"
							>
								{setup.secret.replace(/(.{4})/g, "$1 ").trim()}
							</Badge>
						) : (
							<Skeleton className="h-6 w-40" />
						)}
					</div>
				</div>
				<FormError message={error} />
				{!setup && error && !busy ? (
					<Button type="button" variant="secondary" onClick={fetchSetup}>
						Try again
					</Button>
				) : null}
				<CodeInput id="code" label="Code from your authenticator app" />
				<Button type="submit" size="lg" disabled={busy || !setup}>
					{busy ? "Checking…" : "Turn on two-step sign-in"}
				</Button>
				<Aside>
					<TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
