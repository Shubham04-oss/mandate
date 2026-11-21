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
import { field, useSignIn } from "#/components/auth/use-sign-in.ts";
import { Button } from "#/components/ui/button.tsx";
import { Field, FieldLabel } from "#/components/ui/field.tsx";
import { Input } from "#/components/ui/input.tsx";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";

export const Route = createFileRoute("/forgot-password")({
	head: () => ({ meta: [{ title: `Reset your password · ${brand.name}` }] }),
	component: ForgotPassword,
});

function ForgotPassword() {
	const [sentTo, setSentTo] = useState<string | null>(null);
	const { busy, error, act } = useSignIn();

	return (
		<AuthLayout scene="deck">
			<AuthForm
				label="Reset your password"
				onSubmit={(data) => {
					const email = field(data, "email");
					act(
						() => api.auth.forgotPassword(email),
						() => setSentTo(email),
					);
				}}
			>
				<StatusIcon icon="key" />
				<Title>Reset your password</Title>
				<Lead>We'll send you a link to choose a new one.</Lead>
				{/* The core answers the same way whether or not the email has an account (4.1). */}
				<FormNote>
					{sentTo
						? `If ${sentTo} has an account, a reset link is on its way. It works for a short while.`
						: null}
				</FormNote>
				<FormError message={error} />
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
				<Button type="submit" size="lg" disabled={busy}>
					{busy ? "Sending…" : sentTo ? "Send it again" : "Send reset link"}
				</Button>
				<Aside>
					Remembered it? <TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
