import { createFileRoute } from "@tanstack/react-router";
import {
	Aside,
	AuthForm,
	AuthLayout,
	Lead,
	StatusIcon,
	TextLink,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { CodeInput } from "#/components/auth/parts.tsx";
import { Badge } from "#/components/ui/badge.tsx";
import { Button, buttonVariants } from "#/components/ui/button.tsx";
import { brand } from "#/lib/brand.ts";
import { sample } from "#/lib/sample.ts";

export const Route = createFileRoute("/verify-email")({
	head: () => ({ meta: [{ title: `Check your inbox · ${brand.name}` }] }),
	component: VerifyEmail,
});

// WorkOS verifies an email with a six-digit code rather than a link.
function VerifyEmail() {
	return (
		<AuthLayout scene="bottles">
			<AuthForm label="Verify your email">
				<StatusIcon icon="mail" />
				<Title>Check your inbox</Title>
				<Lead>
					We sent a 6-digit code to{" "}
					<Badge variant="secondary">{sample.email}</Badge>. Enter it to finish
					signing up.
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
				<CodeInput id="code" label="Verification code" />
				<Button type="submit" size="lg">
					Verify email
				</Button>
				<Aside>
					No email after a minute?{" "}
					<TextLink to="/verify-email">Send it again</TextLink> ·{" "}
					<TextLink to="/sign-up">Use another email</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
