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

export const Route = createFileRoute("/forgot-password")({
	head: () => ({ meta: [{ title: `Reset your password · ${brand.name}` }] }),
	component: ForgotPassword,
});

function ForgotPassword() {
	return (
		<AuthLayout scene="deck">
			<AuthForm label="Reset your password">
				<StatusIcon icon="key" />
				<Title>Reset your password</Title>
				<Lead>We'll send you a link to choose a new one.</Lead>
				<Field className="gap-1.5">
					<FieldLabel htmlFor="email">Email</FieldLabel>
					<Input
						id="email"
						name="email"
						type="email"
						autoComplete="email"
						placeholder="you@company.com"
					/>
				</Field>
				<Button type="submit" size="lg">
					Send reset link
				</Button>
				<Aside>
					Remembered it? <TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
