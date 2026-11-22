import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import {
	Aside,
	AuthForm,
	AuthLayout,
	FormError,
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
import { readFlow } from "#/lib/sign-in-flow.ts";

// The core finds the organization's connection from the email's domain and hands back the identity
// provider's address; the provider returns to the core, which sets the session and sends the person on.
export const Route = createFileRoute("/sso")({
	head: () => ({ meta: [{ title: `Single sign-on · ${brand.name}` }] }),
	ssr: false,
	component: Sso,
});

function Sso() {
	const [email] = useState(() => readFlow().email ?? "");
	const { busy, error, act } = useSignIn();

	return (
		<AuthLayout scene="harbour">
			<AuthForm
				label="Sign in with single sign-on"
				onSubmit={(data) =>
					act(
						() => api.auth.ssoStart(field(data, "email")),
						({ redirect_url }) => window.location.assign(redirect_url),
					)
				}
			>
				<StatusIcon icon="building" />
				<Title>Single sign-on</Title>
				<Lead>
					Enter your work email and we'll take you to your company's sign-in.
				</Lead>
				<FormError message={error} />
				<Field className="gap-1.5">
					<FieldLabel htmlFor="email">Work email</FieldLabel>
					<Input
						id="email"
						name="email"
						autoFocus
						type="email"
						autoComplete="email"
						placeholder="you@company.com"
						defaultValue={email}
					/>
				</Field>
				<Button type="submit" size="lg" disabled={busy}>
					{busy ? "Finding your sign-in…" : "Continue"}
				</Button>
				<Aside>
					<TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
