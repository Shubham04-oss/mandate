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
import { CodeInput } from "#/components/auth/parts.tsx";
import { StartAgain } from "#/components/auth/start-again.tsx";
import { field, useSignIn } from "#/components/auth/use-sign-in.ts";
import { Button } from "#/components/ui/button.tsx";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { readFlow } from "#/lib/sign-in-flow.ts";

// `next: mfa`: a password sign-in for someone with two-step on.
export const Route = createFileRoute("/mfa")({
	head: () => ({ meta: [{ title: `Two-step sign-in · ${brand.name}` }] }),
	ssr: false,
	component: MfaChallenge,
});

function MfaChallenge() {
	const [pending] = useState(() => readFlow().pending);
	const { busy, error, run } = useSignIn();

	if (!pending) return <StartAgain />;

	return (
		<AuthLayout scene="crows-nest">
			<AuthForm
				label="Enter your authenticator code"
				onSubmit={(data) =>
					run(() => api.auth.mfaVerify(pending, field(data, "code")))
				}
			>
				<StatusIcon icon="shield-check" />
				<Title>Enter your code</Title>
				<Lead>
					Open your authenticator app and enter the 6-digit code for{" "}
					{brand.name}.
				</Lead>
				<FormError message={error} />
				<CodeInput id="code" label="Authenticator code" autoFocus />
				<Button type="submit" size="lg" disabled={busy}>
					{busy ? "Checking…" : "Verify"}
				</Button>
				<Aside>
					Lost your device? Ask your admin to reset it.
					<br />
					<TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
