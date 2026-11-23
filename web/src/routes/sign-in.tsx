import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import {
	Aside,
	AuthForm,
	AuthLayout,
	FormError,
	FormNote,
	Lead,
	SceneCard,
	TextLink,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { GoogleButton, PasswordInput } from "#/components/auth/parts.tsx";
import { field, useSignIn } from "#/components/auth/use-sign-in.ts";
import { Button } from "#/components/ui/button.tsx";
import { Field, FieldLabel, FieldSeparator } from "#/components/ui/field.tsx";
import { Input } from "#/components/ui/input.tsx";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { callbackMessage, saveFlow } from "#/lib/sign-in-flow.ts";

type Search = { error?: string; reset?: boolean };

export const Route = createFileRoute("/sign-in")({
	head: () => ({ meta: [{ title: `Sign in · ${brand.name}` }] }),
	// `error` comes back from the core's SSO callback; `reset` after a new password is saved.
	validateSearch: (s: Record<string, unknown>): Search => ({
		error: typeof s.error === "string" ? s.error : undefined,
		reset: s.reset === true || s.reset === "1" ? true : undefined,
	}),
	component: SignIn,
});

function SignIn() {
	const search = Route.useSearch();
	const navigate = useNavigate();
	const { busy, error, setError, run, act } = useSignIn();
	const [email, setEmail] = useState("");

	const emailMeACode = () => {
		if (!email.trim())
			return setError("Enter your email first, then we'll send a code to it.");
		act(
			() => api.auth.codeStart(email.trim()),
			() => {
				saveFlow({ email: email.trim() });
				return navigate({ to: "/sign-in/code" });
			},
		);
	};

	const useSso = () => {
		saveFlow({ email: email.trim() || undefined });
		navigate({ to: "/sso" });
	};

	return (
		<AuthLayout
			scene="bay"
			sceneCard={
				<SceneCard
					avatar="/avatars/agent.svg"
					title="Your agent"
					line="Asks you before it changes anything"
				/>
			}
		>
			<AuthForm
				label="Sign in"
				onSubmit={(data) => {
					const typed = field(data, "email");
					run(
						() => api.auth.password(typed, String(data.get("password") ?? "")),
						{ email: typed },
					);
				}}
			>
				<Title>Welcome back</Title>
				<Lead>Sign in to continue.</Lead>
				{/* The contract has no Google sign-in endpoint yet; see the note for the lead. */}
				<GoogleButton disabled title="Not available yet">
					Continue with Google
				</GoogleButton>
				<FieldSeparator>or with email</FieldSeparator>
				<FormNote>
					{search.reset ? "Your password is updated. Sign in with it." : null}
				</FormNote>
				<FormError message={error ?? callbackMessage(search.error)} />
				<Field className="gap-1.5">
					<FieldLabel htmlFor="email">Email</FieldLabel>
					<Input
						id="email"
						name="email"
						autoFocus
						type="email"
						autoComplete="email"
						placeholder="you@company.com"
						value={email}
						onChange={(e) => setEmail(e.target.value)}
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
				<Button type="submit" size="lg" disabled={busy}>
					{busy ? "Signing in…" : "Sign in"}
				</Button>
				<Aside>
					<button
						type="button"
						onClick={emailMeACode}
						disabled={busy}
						className="cursor-pointer border-0 bg-transparent p-0 font-medium text-muted-foreground hover:underline"
					>
						Email me a code
					</button>{" "}
					·{" "}
					<button
						type="button"
						onClick={useSso}
						className="cursor-pointer border-0 bg-transparent p-0 font-medium text-muted-foreground hover:underline"
					>
						Use single sign-on
					</button>
				</Aside>
				<Aside>
					New to {brand.name}?{" "}
					<TextLink to="/sign-up">Create an account</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
