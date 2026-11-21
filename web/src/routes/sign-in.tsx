import { createFileRoute } from "@tanstack/react-router";
import {
	Aside,
	AuthForm,
	AuthLayout,
	Lead,
	SceneCard,
	TextLink,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { GoogleButton, PasswordInput } from "#/components/auth/parts.tsx";
import { Button } from "#/components/ui/button.tsx";
import { Field, FieldLabel, FieldSeparator } from "#/components/ui/field.tsx";
import { Input } from "#/components/ui/input.tsx";
import { brand } from "#/lib/brand.ts";
import { sample } from "#/lib/sample.ts";

export const Route = createFileRoute("/sign-in")({
	head: () => ({ meta: [{ title: `Sign in · ${brand.name}` }] }),
	component: SignIn,
});

function SignIn() {
	return (
		<AuthLayout
			scene="bay"
			sceneCard={
				<SceneCard
					avatar={sample.agent.avatar}
					title="Your agent"
					line="3 requests are waiting for you"
				/>
			}
		>
			<AuthForm label="Sign in">
				<Title>Welcome back</Title>
				<Lead>Sign in to continue.</Lead>
				<GoogleButton lastUsed>Continue with Google</GoogleButton>
				<FieldSeparator>or with email</FieldSeparator>
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
				<Field className="gap-1.5">
					<div className="flex items-center">
						<FieldLabel htmlFor="password">Password</FieldLabel>
						<span className="ml-auto text-[12px]">
							<TextLink to="/forgot-password" tone="muted">
								Forgot it?
							</TextLink>
						</span>
					</div>
					<PasswordInput
						id="password"
						name="password"
						autoComplete="current-password"
						placeholder="Your password"
					/>
				</Field>
				<Button type="submit" size="lg">
					Sign in
				</Button>
				<Aside>
					<TextLink to="/sign-in/code" tone="muted">
						Email me a code
					</TextLink>{" "}
					·{" "}
					<TextLink to="/sso" tone="muted">
						Use single sign-on
					</TextLink>
				</Aside>
				<Aside>
					New to {brand.name}?{" "}
					<TextLink to="/sign-up">Create an account</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
