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
	CodeInput,
	InviteCard,
	PasswordInput,
} from "#/components/auth/parts.tsx";
import { StartAgain } from "#/components/auth/start-again.tsx";
import { field, useSignIn } from "#/components/auth/use-sign-in.ts";
import { Button } from "#/components/ui/button.tsx";
import { Field, FieldLabel } from "#/components/ui/field.tsx";
import { Input } from "#/components/ui/input.tsx";
import { ApiError, api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { errorMessage } from "#/lib/sign-in-flow.ts";

const gone = { not_found: "This invitation was used or withdrawn." };

// Reached from the invitation email: /invite?token=…
export const Route = createFileRoute("/invite")({
	head: () => ({ meta: [{ title: `Join your team · ${brand.name}` }] }),
	ssr: false,
	validateSearch: (s: Record<string, unknown>): { token?: string } => ({
		token: typeof s.token === "string" ? s.token : undefined,
	}),
	loaderDeps: ({ search }) => ({ token: search.token }),
	loader: async ({ deps }) => {
		if (!deps.token) return { invitation: null, problem: null };
		try {
			return { invitation: await api.auth.invite(deps.token), problem: null };
		} catch (e) {
			return {
				invitation: null,
				problem:
					e instanceof ApiError
						? errorMessage(e, gone)
						: "This invitation can't be opened.",
			};
		}
	},
	component: Invite,
});

/**
 * The person is not signed in here: they accept with a code sent to the invited email, or by setting
 * a name and password for a new account. Both are the contract's two accept bodies.
 */
function Invite() {
	const { token } = Route.useSearch();
	const { invitation, problem } = Route.useLoaderData();
	const [step, setStep] = useState<"start" | "code" | "password">("start");
	const { busy, error, run, act } = useSignIn(gone);

	if (!token) return <StartAgain />;
	if (!invitation) {
		return (
			<AuthLayout scene="harbour">
				<AuthForm label="Invitation">
					<Title>This invitation isn't available</Title>
					<Lead>
						{problem ?? "Ask the person who invited you to send a new one."}
					</Lead>
					<Aside>
						<TextLink to="/sign-in">Go to sign in</TextLink>
					</Aside>
				</AuthForm>
			</AuthLayout>
		);
	}

	const email = invitation.email;
	return (
		<AuthLayout scene="harbour">
			<AuthForm
				label="Join your team"
				onSubmit={(data) =>
					step === "code"
						? run(
								() =>
									api.auth.acceptInvite(token, { code: field(data, "code") }),
								{ email },
							)
						: step === "password"
							? run(
									() =>
										api.auth.acceptInvite(token, {
											name: field(data, "name"),
											password: String(data.get("password") ?? ""),
										}),
									{ email },
								)
							: act(
									() => api.auth.codeStart(email),
									() => setStep("code"),
								)
				}
			>
				<Title>Join your team</Title>
				<Lead>
					Your invitation is ready. Accept it to join the organization.
				</Lead>
				<InviteCard invitation={invitation} />
				<FormError message={error} />

				{step === "code" ? (
					<>
						<Lead>We sent a 6-digit code to {email}.</Lead>
						<CodeInput id="code" label="Code from your email" autoFocus />
					</>
				) : null}
				{step === "password" ? (
					<>
						<Field className="gap-1.5">
							<FieldLabel htmlFor="name">Your name</FieldLabel>
							<Input id="name" name="name" autoComplete="name" />
						</Field>
						<Field className="gap-1.5">
							<FieldLabel htmlFor="password">Choose a password</FieldLabel>
							<PasswordInput
								id="password"
								name="password"
								autoComplete="new-password"
							/>
						</Field>
					</>
				) : null}

				<Button type="submit" size="lg" disabled={busy}>
					{step === "start"
						? busy
							? "Sending…"
							: "Email me a code to join"
						: busy
							? "Joining…"
							: "Accept invitation"}
				</Button>
				<Aside>
					{step === "password" ? (
						<button
							type="button"
							onClick={() => setStep("start")}
							className="cursor-pointer border-0 bg-transparent p-0 font-medium text-muted-foreground hover:underline"
						>
							Use an email code instead
						</button>
					) : (
						<button
							type="button"
							onClick={() => setStep("password")}
							className="cursor-pointer border-0 bg-transparent p-0 font-medium text-muted-foreground hover:underline"
						>
							New here? Set a name and password instead
						</button>
					)}
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
