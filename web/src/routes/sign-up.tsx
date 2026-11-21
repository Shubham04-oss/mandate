import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import {
	Aside,
	AuthForm,
	AuthLayout,
	Lead,
	TextLink,
	Title,
} from "#/components/auth/auth-layout.tsx";
import {
	GoogleButton,
	PasswordInput,
	StrengthMeter,
} from "#/components/auth/parts.tsx";
import { Button } from "#/components/ui/button.tsx";
import {
	Field,
	FieldDescription,
	FieldLabel,
	FieldSeparator,
} from "#/components/ui/field.tsx";
import { Input } from "#/components/ui/input.tsx";
import { brand } from "#/lib/brand.ts";
import { checkPassword } from "#/lib/password.ts";
import { sample } from "#/lib/sample.ts";

export const Route = createFileRoute("/sign-up")({
	head: () => ({ meta: [{ title: `Create an account · ${brand.name}` }] }),
	component: SignUp,
});

function SignUp() {
	const [password, setPassword] = useState("");
	return (
		<AuthLayout scene="crows-nest">
			<AuthForm label="Create an account">
				<Title>Bring your crew aboard</Title>
				<Lead>
					Start free. Your agent, {sample.agent.name}, is ready as soon as
					you're in.
				</Lead>
				<GoogleButton>Sign up with Google</GoogleButton>
				<FieldSeparator>or with email</FieldSeparator>
				<Field className="gap-1.5">
					<FieldLabel htmlFor="name">Your name</FieldLabel>
					<Input
						id="name"
						name="name"
						autoComplete="name"
						placeholder="Ada Brennan"
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
					{(checkPassword(password).hint || undefined) && (
						<FieldDescription id="password-hint" className="text-[11.5px]">
							{checkPassword(password).hint || undefined}
						</FieldDescription>
					)}
				</Field>
				<Button type="submit" size="lg">
					Create account
				</Button>
				<Aside>
					Already aboard? <TextLink to="/sign-in">Sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
