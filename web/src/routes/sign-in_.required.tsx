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
import { GoogleButton } from "#/components/auth/parts.tsx";
import { Icon } from "#/components/icons/icon.tsx";
import { Button } from "#/components/ui/button.tsx";
import { brand } from "#/lib/brand.ts";
import { sample } from "#/lib/sample.ts";

// Shown when WorkOS answers sso_required or organization_authentication_methods_required:
// the person's organization allows only some ways in. The core says which.
export const Route = createFileRoute("/sign-in_/required")({
	head: () => ({ meta: [{ title: `Sign in another way · ${brand.name}` }] }),
	component: RequiredMethod,
});

function RequiredMethod() {
	return (
		<AuthLayout scene="harbour">
			<AuthForm label="Sign in another way">
				<StatusIcon icon="shield-check" />
				<Title>Sign in another way</Title>
				<Lead>
					{sample.org.name} asks its members to sign in with one of these.
				</Lead>
				<GoogleButton>Continue with Google</GoogleButton>
				<Button variant="secondary" size="lg">
					<Icon name="key" />
					Continue with single sign-on
				</Button>
				<Aside>
					<TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
