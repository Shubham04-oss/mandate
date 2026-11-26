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
import { CodeInput } from "#/components/auth/parts.tsx";
import { field, useSignIn } from "#/components/auth/use-sign-in.ts";
import { Badge } from "#/components/ui/badge.tsx";
import { Button } from "#/components/ui/button.tsx";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { readFlow } from "#/lib/sign-in-flow.ts";

// Magic code: the sign-in screen sends the code (`/v1/auth/code/start`) and brings the email here.
export const Route = createFileRoute("/sign-in_/code")({
	head: () => ({ meta: [{ title: `Enter your code · ${brand.name}` }] }),
	ssr: false,
	component: SignInCode,
});

function SignInCode() {
	const [email] = useState(() => readFlow().email ?? "");
	const [sent, setSent] = useState(false);
	const { busy, error, run, act } = useSignIn();

	if (!email) {
		return (
			<AuthLayout scene="bottles">
				<AuthForm label="Sign in with a code">
					<Title>Start from sign-in</Title>
					<Lead>
						Enter your email on the sign-in page and choose “Email me a code”.
					</Lead>
					<Aside>
						<TextLink to="/sign-in">Back to sign in</TextLink>
					</Aside>
				</AuthForm>
			</AuthLayout>
		);
	}

	return (
		<AuthLayout scene="bottles">
			<AuthForm
				label="Sign in with a code"
				onSubmit={(data) =>
					run(() => api.auth.codeVerify(email, field(data, "code")), { email })
				}
			>
				<StatusIcon icon="mail" />
				<Title>Enter your code</Title>
				<Lead>
					We sent a 6-digit code to <Badge variant="secondary">{email}</Badge>.
					It works for 10 minutes.
				</Lead>
				<FormNote>{sent ? "We sent a new code." : null}</FormNote>
				<FormError message={error} />
				<CodeInput id="code" label="Sign-in code" autoFocus />
				<Button type="submit" size="lg" disabled={busy}>
					{busy ? "Checking…" : "Sign in"}
				</Button>
				<Aside>
					No email after a minute?{" "}
					<button
						type="button"
						disabled={busy}
						onClick={() =>
							act(
								() => api.auth.codeStart(email),
								() => setSent(true),
							)
						}
						className="cursor-pointer border-0 bg-transparent p-0 font-semibold text-foreground hover:underline"
					>
						Send it again
					</button>{" "}
					· <TextLink to="/sign-in">Use your password</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
