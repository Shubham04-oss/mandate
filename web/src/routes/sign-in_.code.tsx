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

// WorkOS Magic Auth: a one-time code by email instead of a password.
export const Route = createFileRoute("/sign-in_/code")({
	head: () => ({ meta: [{ title: `Enter your code · ${brand.name}` }] }),
	component: SignInCode,
});

function SignInCode() {
	return (
		<AuthLayout scene="bottles">
			<AuthForm label="Sign in with a code">
				<StatusIcon icon="mail" />
				<Title>Enter your code</Title>
				<Lead>
					We sent a 6-digit code to{" "}
					<Badge variant="secondary">{sample.email}</Badge>. It works for 10
					minutes.
				</Lead>
				<CodeInput id="code" label="Sign-in code" autoFocus />
				<Button type="submit" size="lg">
					Sign in
				</Button>
				<Aside>
					No email after a minute?{" "}
					<TextLink to="/sign-in/code">Send it again</TextLink> ·{" "}
					<TextLink to="/sign-in">Use your password</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
