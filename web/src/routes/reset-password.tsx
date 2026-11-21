import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import {
	AuthForm,
	AuthLayout,
	Lead,
	StatusIcon,
	Title,
} from "#/components/auth/auth-layout.tsx";
import {
	PasswordInput,
	PasswordRules,
	StrengthMeter,
} from "#/components/auth/parts.tsx";
import { Button } from "#/components/ui/button.tsx";
import { Field, FieldLabel } from "#/components/ui/field.tsx";
import { brand } from "#/lib/brand.ts";

// Reached from the link in the reset email; the link's token goes to the core with the password.
export const Route = createFileRoute("/reset-password")({
	head: () => ({ meta: [{ title: `Choose a new password · ${brand.name}` }] }),
	component: ResetPassword,
});

function ResetPassword() {
	const [password, setPassword] = useState("");
	return (
		<AuthLayout scene="deck">
			<AuthForm label="Choose a new password">
				<StatusIcon icon="lock" />
				<Title>Choose a new password</Title>
				<Lead>You'll be signed out everywhere else once it's saved.</Lead>
				<Field className="gap-1.5">
					<FieldLabel htmlFor="password">New password</FieldLabel>
					<PasswordInput
						id="password"
						name="password"
						autoComplete="new-password"
						value={password}
						onChange={(e) => setPassword(e.target.value)}
					/>
					<StrengthMeter password={password} />
				</Field>
				<PasswordRules password={password} />
				<Button type="submit" size="lg">
					Update password
				</Button>
			</AuthForm>
		</AuthLayout>
	);
}
