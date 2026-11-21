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
import { Button } from "#/components/ui/button.tsx";
import { brand } from "#/lib/brand.ts";
import { sample } from "#/lib/sample.ts";

// WorkOS mfa_enrollment: the organization requires two-step sign-in and the person has none yet.
// WorkOS returns the QR image and the secret; the core passes them through.
export const Route = createFileRoute("/mfa_/setup")({
	head: () => ({
		meta: [{ title: `Set up two-step sign-in · ${brand.name}` }],
	}),
	component: MfaSetup,
});

function MfaSetup() {
	return (
		<AuthLayout scene="crows-nest">
			<AuthForm label="Set up two-step sign-in">
				<StatusIcon icon="shield-check" />
				<Title>Set up two-step sign-in</Title>
				<Lead>
					{sample.org.name} asks for a code from an authenticator app. Scan this
					with one, then enter the code it shows.
				</Lead>
				<div className="flex items-center gap-4 rounded-section bg-raised p-3 shadow-raised">
					<img
						src={sample.mfa.qr}
						alt="QR code for your authenticator app"
						width={112}
						height={112}
						className="shrink-0 rounded-item bg-white p-2"
					/>
					<div className="flex flex-col gap-2 text-[12px] text-foreground-muted leading-normal">
						<span>Can't scan it? Enter this key instead.</span>
						<span>
							<Badge
								variant="secondary"
								className="font-mono text-[12px] tracking-wide"
							>
								{sample.mfa.secret}
							</Badge>
						</span>
					</div>
				</div>
				<CodeInput id="code" label="Code from your authenticator app" />
				<Button type="submit" size="lg">
					Turn on two-step sign-in
				</Button>
				<Aside>
					<TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
