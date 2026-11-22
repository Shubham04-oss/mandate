import { createFileRoute, useNavigate } from "@tanstack/react-router";
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
import { GoogleButton } from "#/components/auth/parts.tsx";
import { StartAgain } from "#/components/auth/start-again.tsx";
import { useSignIn } from "#/components/auth/use-sign-in.ts";
import { Icon } from "#/components/icons/icon.tsx";
import { Button } from "#/components/ui/button.tsx";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { readFlow, saveFlow } from "#/lib/sign-in-flow.ts";

// `next: method_required`: the organization allows only some ways in, listed in `methods`. The core
// passes WorkOS's method names through as they are.
// ponytail: only the ways the web app can offer are shown; others (Microsoft, GitHub, Apple,
// passkeys) get a note instead.
const offered = ["google_oauth", "sso", "magic_auth", "password"];
export const Route = createFileRoute("/sign-in_/required")({
	head: () => ({ meta: [{ title: `Sign in another way · ${brand.name}` }] }),
	ssr: false,
	component: RequiredMethod,
});

function RequiredMethod() {
	const [flow] = useState(readFlow);
	const navigate = useNavigate();
	const { busy, error, act } = useSignIn();
	const methods = flow.methods ?? [];

	if (!methods.length) return <StartAgain />;

	const email = flow.email;
	const none = !methods.some((m) => offered.includes(m));
	return (
		<AuthLayout scene="harbour">
			<AuthForm label="Sign in another way">
				<StatusIcon icon="shield-check" />
				<Title>Sign in another way</Title>
				<Lead>
					Your organization asks its members to sign in with one of these.
				</Lead>
				<FormError message={error} />
				{none ? (
					<FormNote>
						oryxa can't offer the way your organization signs in yet. Ask your
						admin which other ways are allowed.
					</FormNote>
				) : null}
				{methods.includes("google_oauth") ? (
					<GoogleButton disabled title="Not available yet">
						Continue with Google
					</GoogleButton>
				) : null}
				{methods.includes("sso") ? (
					<Button
						variant="secondary"
						size="lg"
						onClick={() => {
							saveFlow({ email });
							navigate({ to: "/sso" });
						}}
					>
						<Icon name="key" />
						Continue with single sign-on
					</Button>
				) : null}
				{methods.includes("magic_auth") && email ? (
					<Button
						variant="secondary"
						size="lg"
						disabled={busy}
						onClick={() =>
							act(
								() => api.auth.codeStart(email),
								() => {
									saveFlow({ email });
									return navigate({ to: "/sign-in/code" });
								},
							)
						}
					>
						<Icon name="mail" />
						Email me a sign-in code
					</Button>
				) : null}
				{methods.includes("password") ? (
					<Button
						variant="secondary"
						size="lg"
						onClick={() => navigate({ to: "/sign-in" })}
					>
						<Icon name="lock" />
						Sign in with your password
					</Button>
				) : null}
				<Aside>
					<TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
