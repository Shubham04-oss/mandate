import { createFileRoute, Link } from "@tanstack/react-router";
import {
	Aside,
	AuthLayout,
	AuthStatus,
	Lead,
	StatusIcon,
	TextLink,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { Badge } from "#/components/ui/badge.tsx";
import { buttonVariants } from "#/components/ui/button.tsx";
import { brand } from "#/lib/brand.ts";
import { sample } from "#/lib/sample.ts";

// WorkOS proved who the person is, but the core found no organization for them.
export const Route = createFileRoute("/no-access")({
	head: () => ({ meta: [{ title: `No access yet · ${brand.name}` }] }),
	component: NoAccess,
});

function NoAccess() {
	return (
		<AuthLayout scene="gate">
			<AuthStatus label="No access yet" alert>
				<StatusIcon icon="cross-circle" tone="danger" />
				<Title>No access yet</Title>
				<Lead>
					<Badge variant="secondary">{sample.email}</Badge> isn't part of an
					organization on {brand.name}. Ask your admin for an invitation, or
					sign in with another account.
				</Lead>
				<Link to="/sign-in" className={buttonVariants({ size: "lg" })}>
					Use another account
				</Link>
				<Aside>
					Setting up for your team?{" "}
					<TextLink to="/sign-up">Create an account</TextLink>
				</Aside>
			</AuthStatus>
		</AuthLayout>
	);
}
