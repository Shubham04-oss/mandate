import { createFileRoute } from "@tanstack/react-router";
import {
	Aside,
	AuthForm,
	AuthLayout,
	Lead,
	TextLink,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { Icon } from "#/components/icons/icon.tsx";
import { Alert, AlertDescription } from "#/components/ui/alert.tsx";
import { Button } from "#/components/ui/button.tsx";
import { brand } from "#/lib/brand.ts";
import { sample } from "#/lib/sample.ts";

export const Route = createFileRoute("/link-expired")({
	head: () => ({ meta: [{ title: `Link not available · ${brand.name}` }] }),
	component: LinkExpired,
});

function LinkExpired() {
	return (
		<AuthLayout scene="gate">
			<AuthForm label="This link isn't available">
				<Title>This link isn't available</Title>
				<Lead>Reset links expire, and each one works once.</Lead>
				<Alert>
					<Icon name="clock" />
					<AlertDescription>
						We can send a fresh link to{" "}
						<b className="text-foreground">{sample.email}</b>. It replaces the
						old one.
					</AlertDescription>
				</Alert>
				<Button type="submit" size="lg">
					Send a new link
				</Button>
				<Aside>
					<TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
