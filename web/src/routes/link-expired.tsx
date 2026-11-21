import { createFileRoute, Link } from "@tanstack/react-router";
import {
	Aside,
	AuthLayout,
	AuthStatus,
	Lead,
	TextLink,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { Icon } from "#/components/icons/icon.tsx";
import { Alert, AlertDescription } from "#/components/ui/alert.tsx";
import { buttonVariants } from "#/components/ui/button.tsx";
import { brand } from "#/lib/brand.ts";

// The reset link was used or expired. The contract's reset request needs the email, which an expired
// token can't give back, so the person types it again on the forgot-password screen.
export const Route = createFileRoute("/link-expired")({
	head: () => ({ meta: [{ title: `Link not available · ${brand.name}` }] }),
	component: LinkExpired,
});

function LinkExpired() {
	return (
		<AuthLayout scene="gate">
			<AuthStatus label="This link isn't available" alert>
				<Title>This link isn't available</Title>
				<Lead>Reset links expire, and each one works once.</Lead>
				<Alert>
					<Icon name="clock" />
					<AlertDescription>
						A fresh link replaces the old one. It takes a few seconds.
					</AlertDescription>
				</Alert>
				<Link to="/forgot-password" className={buttonVariants({ size: "lg" })}>
					Get a new link
				</Link>
				<Aside>
					<TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthStatus>
		</AuthLayout>
	);
}
