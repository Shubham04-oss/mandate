import { createFileRoute } from "@tanstack/react-router";
import {
	AuthForm,
	AuthLayout,
	Lead,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { InviteCard, SignedInAs } from "#/components/auth/parts.tsx";
import { Button } from "#/components/ui/button.tsx";
import { brand } from "#/lib/brand.ts";
import { sample } from "#/lib/sample.ts";

export const Route = createFileRoute("/invite")({
	head: () => ({ meta: [{ title: `Join your team · ${brand.name}` }] }),
	component: Invite,
});

function Invite() {
	return (
		<AuthLayout scene="harbour">
			<AuthForm label="Join your team">
				<Title>Join your team</Title>
				<Lead>Your invitation is ready. Accept it to enter the workspace.</Lead>
				<InviteCard org={sample.org} />
				<Button type="submit" size="lg">
					Accept invitation
				</Button>
				<SignedInAs email={sample.email} avatar={sample.me.avatar} />
			</AuthForm>
		</AuthLayout>
	);
}
