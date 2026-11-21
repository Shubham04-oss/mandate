import { createFileRoute } from "@tanstack/react-router";
import {
	AuthForm,
	AuthLayout,
	Lead,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { OrgChoice, SignedInAs } from "#/components/auth/parts.tsx";
import { brand } from "#/lib/brand.ts";
import { sample } from "#/lib/sample.ts";

// WorkOS answers organization_selection_required when a person belongs to several. The choice goes
// back to the core with the pending token; WorkOS and the core check it, the web app does not.
export const Route = createFileRoute("/choose-organization")({
	head: () => ({ meta: [{ title: `Choose an organization · ${brand.name}` }] }),
	component: ChooseOrganization,
});

function ChooseOrganization() {
	return (
		<AuthLayout scene="harbour">
			<AuthForm label="Choose an organization">
				<Title>Choose an organization</Title>
				<Lead>You belong to more than one. You can switch later.</Lead>
				<div className="flex flex-col gap-2">
					{sample.orgs.map((org) => (
						<OrgChoice
							key={org.id}
							name={org.name}
							initial={org.initial}
							summary={org.summary}
						/>
					))}
				</div>
				<SignedInAs email={sample.email} avatar={sample.me.avatar} />
			</AuthForm>
		</AuthLayout>
	);
}
