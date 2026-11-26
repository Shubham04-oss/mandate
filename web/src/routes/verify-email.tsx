import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import {
	Aside,
	AuthForm,
	AuthLayout,
	FormError,
	FormNote,
	Lead,
	StatusIcon,
	TextLink,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { CodeInput } from "#/components/auth/parts.tsx";
import { StartAgain } from "#/components/auth/start-again.tsx";
import { field, useSignIn } from "#/components/auth/use-sign-in.ts";
import { Badge } from "#/components/ui/badge.tsx";
import { Button, buttonVariants } from "#/components/ui/button.tsx";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { readFlow } from "#/lib/sign-in-flow.ts";

// Reached from sign-up, or from any sign-in whose email isn't verified yet (`next: verify_email`).
export const Route = createFileRoute("/verify-email")({
	head: () => ({ meta: [{ title: `Check your inbox · ${brand.name}` }] }),
	ssr: false,
	component: VerifyEmail,
});

function VerifyEmail() {
	const [flow] = useState(readFlow);
	const [resent, setResent] = useState(false);
	const { busy, error, run, act } = useSignIn();
	const pending = flow.pending;

	if (!pending) return <StartAgain />;

	return (
		<AuthLayout scene="bottles">
			<AuthForm
				label="Verify your email"
				onSubmit={(data) =>
					run(() => api.auth.verifyEmail(pending, field(data, "code")), {
						signedInTo: "/email-verified",
					})
				}
			>
				<StatusIcon icon="mail" />
				<Title>Check your inbox</Title>
				<Lead>
					We sent a 6-digit code to{" "}
					{flow.email ? (
						<Badge variant="secondary">{flow.email}</Badge>
					) : (
						"your email"
					)}
					. Enter it to finish signing up.
				</Lead>
				<div className="flex gap-2">
					<a
						href="https://mail.google.com"
						target="_blank"
						rel="noreferrer"
						className={buttonVariants({
							variant: "secondary",
							size: "md",
							className: "flex-1",
						})}
					>
						Open Gmail
					</a>
					<a
						href="https://outlook.live.com"
						target="_blank"
						rel="noreferrer"
						className={buttonVariants({
							variant: "secondary",
							size: "md",
							className: "flex-1",
						})}
					>
						Open Outlook
					</a>
				</div>
				<FormNote>{resent ? "We sent a new code." : null}</FormNote>
				<FormError message={error} />
				<CodeInput id="code" label="Verification code" />
				<Button type="submit" size="lg" disabled={busy}>
					{busy ? "Checking…" : "Verify email"}
				</Button>
				<Aside>
					No email after a minute?{" "}
					<button
						type="button"
						disabled={busy}
						onClick={() =>
							act(
								() => api.auth.resendVerification(pending),
								() => setResent(true),
							)
						}
						className="cursor-pointer border-0 bg-transparent p-0 font-semibold text-foreground hover:underline"
					>
						Send it again
					</button>
				</Aside>
				{/* Worded the same whatever happened: the core never says whether the email has an account. */}
				<Aside>
					Didn't get a code? Check your email address and password, then{" "}
					<TextLink to="/sign-up">sign up again</TextLink>.
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
