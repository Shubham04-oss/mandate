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
import { Button } from "#/components/ui/button.tsx";
import { brand } from "#/lib/brand.ts";

// WorkOS mfa_challenge: an enrolled person confirms sign-in with their authenticator code.
export const Route = createFileRoute("/mfa")({
	head: () => ({ meta: [{ title: `Two-step sign-in · ${brand.name}` }] }),
	component: MfaChallenge,
});

function MfaChallenge() {
	return (
		<AuthLayout scene="crows-nest">
			<AuthForm label="Enter your authenticator code">
				<StatusIcon icon="shield-check" />
				<Title>Enter your code</Title>
				<Lead>
					Open your authenticator app and enter the 6-digit code for{" "}
					{brand.name}.
				</Lead>
				<CodeInput id="code" label="Authenticator code" autoFocus />
				<Button type="submit" size="lg">
					Verify
				</Button>
				<Aside>
					Lost your device? Ask your workspace admin to reset it.
					<br />
					<TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
