import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import {
	Aside,
	AuthForm,
	AuthLayout,
	FormError,
	Lead,
	TextLink,
	Title,
} from "#/components/auth/auth-layout.tsx";
import {
	GoogleButton,
	PasswordInput,
	StrengthMeter,
} from "#/components/auth/parts.tsx";
import { field, useSignIn } from "#/components/auth/use-sign-in.ts";
import { Button } from "#/components/ui/button.tsx";
import {
	Field,
	FieldDescription,
	FieldLabel,
	FieldSeparator,
} from "#/components/ui/field.tsx";
import { Input } from "#/components/ui/input.tsx";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { checkPassword } from "#/lib/password.ts";

export const Route = createFileRoute("/sign-up")({
	head: () => ({ meta: [{ title: `Create an account · ${brand.name}` }] }),
	component: SignUp,
});

function SignUp() {
	const [password, setPassword] = useState("");
	const { busy, error, run } = useSignIn();
	const hint = checkPassword(password).hint;

	return (
		<AuthLayout scene="crows-nest">
			<AuthForm
				label="Create an account"
				onSubmit={(data) => {
					const email = field(data, "email");
					run(
						() =>
							api.auth.signUp({
								email,
								password,
								name: field(data, "name"),
								orgName: field(data, "org"),
							}),
						{ email },
					);
				}}
			>
				<Title>Create your account</Title>
				<Lead>Your agent is ready as soon as you're in.</Lead>
				<GoogleButton disabled title="Not available yet">
					Sign up with Google
				</GoogleButton>
				<FieldSeparator>or with email</FieldSeparator>
				<FormError message={error} />
				<Field className="gap-1.5">
					<FieldLabel htmlFor="name">Your name</FieldLabel>
					<Input
						id="name"
						name="name"
						autoComplete="name"
						autoFocus
						placeholder="First and last name"
					/>
				</Field>
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
				<Field className="gap-1.5">
					<FieldLabel htmlFor="org">Organization</FieldLabel>
					<Input
						id="org"
						name="org"
						autoComplete="organization"
						placeholder="Your company"
						aria-describedby="org-hint"
					/>
					<FieldDescription id="org-hint" className="text-[11.5px]">
						You'll be its admin, and can invite your team once you're in.
					</FieldDescription>
				</Field>
				<Field className="gap-1.5">
					<FieldLabel htmlFor="password">Password</FieldLabel>
					<PasswordInput
						id="password"
						name="password"
						autoComplete="new-password"
						aria-describedby="password-hint"
						value={password}
						onChange={(e) => setPassword(e.target.value)}
					/>
					<StrengthMeter password={password} />
					{hint ? (
						<FieldDescription id="password-hint" className="text-[11.5px]">
							{hint}
						</FieldDescription>
					) : null}
				</Field>
				<Button type="submit" size="lg" disabled={busy}>
					{busy ? "Creating…" : "Create account"}
				</Button>
				<Aside>
					Already have an account? <TextLink to="/sign-in">Sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
