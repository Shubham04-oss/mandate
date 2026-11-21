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
import { Button } from "#/components/ui/button.tsx";
import { Field, FieldLabel } from "#/components/ui/field.tsx";
import { Input } from "#/components/ui/input.tsx";
import { brand } from "#/lib/brand.ts";

// Enterprise SSO: the core finds the organization's connection from the email's domain.
export const Route = createFileRoute("/sso")({
	head: () => ({ meta: [{ title: `Single sign-on · ${brand.name}` }] }),
	component: Sso,
});

function Sso() {
	return (
		<AuthLayout scene="harbour">
			<AuthForm label="Sign in with single sign-on">
				<StatusIcon icon="building" />
				<Title>Single sign-on</Title>
				<Lead>
					Enter your work email and we'll take you to your company's sign-in.
				</Lead>
				<Field className="gap-1.5">
					<FieldLabel htmlFor="email">Work email</FieldLabel>
					<Input
						id="email"
						name="email"
						type="email"
						autoComplete="email"
						placeholder="you@company.com"
					/>
				</Field>
				<Button type="submit" size="lg">
					Continue
				</Button>
				<Aside>
					<TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
