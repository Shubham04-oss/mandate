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
	FieldError,
	FieldLabel,
	FieldSeparator,
} from "#/components/ui/field.tsx";
import { Input } from "#/components/ui/input.tsx";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import {
	checkPassword,
	minPassword,
	type SignUpField,
	signUpProblems,
} from "#/lib/password.ts";

export const Route = createFileRoute("/sign-up")({
	head: () => ({ meta: [{ title: `Create an account · ${brand.name}` }] }),
	component: SignUp,
});

function SignUp() {
	const [password, setPassword] = useState("");
	const { busy, error, run } = useSignIn();
	const [problems, setProblems] = useState<
		Partial<Record<SignUpField, string>>
	>({});
	const hint = checkPassword(password).hint;
	// The error (if any) and the hint, for aria-describedby.
	const describedBy = (f: SignUpField, extra?: string) =>
		[problems[f] ? `${f}-error` : null, extra].filter(Boolean).join(" ") ||
		undefined;

	return (
		<AuthLayout scene="crows-nest">
			<AuthForm
				label="Create an account"
				onSubmit={(data) => {
					const email = field(data, "email");
					const found = signUpProblems({
						name: field(data, "name"),
						email,
						org: field(data, "org"),
						password,
					});
					setProblems(found);
					const first = (["name", "email", "org", "password"] as const).find(
						(f) => found[f],
					);
					if (first) {
						document.getElementById(first)?.focus();
						return;
					}
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
						required
						placeholder="First and last name"
						aria-invalid={problems.name ? true : undefined}
						aria-describedby={describedBy("name")}
					/>
					<FieldError id="name-error" className="text-[11.5px]">
						{problems.name}
					</FieldError>
				</Field>
				<Field className="gap-1.5">
					<FieldLabel htmlFor="email">Work email</FieldLabel>
					<Input
						id="email"
						name="email"
						type="email"
						autoComplete="email"
						required
						placeholder="you@company.com"
						aria-invalid={problems.email ? true : undefined}
						aria-describedby={describedBy("email")}
					/>
					<FieldError id="email-error" className="text-[11.5px]">
						{problems.email}
					</FieldError>
				</Field>
				<Field className="gap-1.5">
					<FieldLabel htmlFor="org">Organization</FieldLabel>
					<Input
						id="org"
						name="org"
						autoComplete="organization"
						required
						placeholder="Your company"
						aria-invalid={problems.org ? true : undefined}
						aria-describedby={describedBy("org", "org-hint")}
					/>
					<FieldError id="org-error" className="text-[11.5px]">
						{problems.org}
					</FieldError>
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
						required
						minLength={minPassword}
						aria-invalid={problems.password ? true : undefined}
						aria-describedby={describedBy(
							"password",
							hint ? "password-hint" : undefined,
						)}
						value={password}
						onChange={(e) => setPassword(e.target.value)}
					/>
					<StrengthMeter password={password} />
					<FieldError id="password-error" className="text-[11.5px]">
						{problems.password}
					</FieldError>
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
