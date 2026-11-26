import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import {
	AuthForm,
	AuthLayout,
	FormError,
	Lead,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { OrgChoice, SignedInAs } from "#/components/auth/parts.tsx";
import { StartAgain } from "#/components/auth/start-again.tsx";
import { useSignIn } from "#/components/auth/use-sign-in.ts";
import { api } from "#/lib/api/index.ts";
import { brand } from "#/lib/brand.ts";
import { readFlow } from "#/lib/sign-in-flow.ts";

// `next: choose_org`: the person belongs to several organizations. Switching organization later
// signs in again through here too (4.2).
export const Route = createFileRoute("/choose-organization")({
	head: () => ({ meta: [{ title: `Choose an organization · ${brand.name}` }] }),
	ssr: false,
	component: ChooseOrganization,
});

function ChooseOrganization() {
	const [flow] = useState(readFlow);
	const { busy, error, run } = useSignIn();
	const { pending, orgs } = flow;

	if (!pending || !orgs?.length) return <StartAgain />;

	return (
		<AuthLayout scene="harbour">
			<AuthForm label="Choose an organization">
				<Title>Choose an organization</Title>
				<Lead>You belong to more than one.</Lead>
				<FormError message={error} />
				<div className="flex flex-col gap-2">
					{orgs.map((org) => (
						<OrgChoice
							key={org.id}
							org={org}
							disabled={busy}
							onSelect={() => run(() => api.auth.chooseOrg(pending, org.id))}
						/>
					))}
				</div>
				{flow.email ? <SignedInAs email={flow.email} /> : null}
			</AuthForm>
		</AuthLayout>
	);
}
