import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import {
	AuthForm,
	AuthLayout,
	FormError,
	Lead,
	StatusIcon,
	Title,
} from "#/components/auth/auth-layout.tsx";
import {
	PasswordInput,
	PasswordRules,
	StrengthMeter,
} from "#/components/auth/parts.tsx";
import { StartAgain } from "#/components/auth/start-again.tsx";
import { Button } from "#/components/ui/button.tsx";
import { Field, FieldLabel } from "#/components/ui/field.tsx";
import { ApiError, api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";

// Reached from the link in the reset email: /reset-password?token=…
export const Route = createFileRoute("/reset-password")({
	head: () => ({ meta: [{ title: `Choose a new password · ${brand.name}` }] }),
	validateSearch: (s: Record<string, unknown>): { token?: string } => ({
		token: typeof s.token === "string" ? s.token : undefined,
	}),
	component: ResetPassword,
});

function ResetPassword() {
	const { token } = Route.useSearch();
	const navigate = useNavigate();
	const [password, setPassword] = useState("");
	const [busy, setBusy] = useState(false);
	const [error, setError] = useState<string | null>(null);

	if (!token) return <StartAgain />;

	const save = async () => {
		setBusy(true);
		setError(null);
		try {
			await api.auth.resetPassword(token, password);
			await navigate({ to: "/sign-in", search: { reset: true } });
		} catch (e) {
			// ponytail: the contract doesn't name the error for a used or expired token; code_expired and
			// not_found are taken to mean it (noted for the lead).
			if (
				e instanceof ApiError &&
				(e.code === "code_expired" || e.code === "not_found")
			) {
				await navigate({ to: "/link-expired" });
			} else {
				setError(
					e instanceof ApiError
						? e.message
						: "Something went wrong. Try again.",
				);
			}
		} finally {
			setBusy(false);
		}
	};

	return (
		<AuthLayout scene="deck">
			<AuthForm label="Choose a new password" onSubmit={save}>
				<StatusIcon icon="lock" />
				<Title>Choose a new password</Title>
				<Lead>You'll be signed out everywhere else once it's saved.</Lead>
				<FormError message={error} />
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
				<Button type="submit" size="lg" disabled={busy}>
					{busy ? "Saving…" : "Update password"}
				</Button>
			</AuthForm>
		</AuthLayout>
	);
}
