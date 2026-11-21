import { createFileRoute, Link } from "@tanstack/react-router";
import { useState } from "react";
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
import { clearFlow, readFlow } from "#/lib/sign-in-flow.ts";

// WorkOS proved who the person is, but the core found no organization for them.
export const Route = createFileRoute("/no-access")({
	head: () => ({ meta: [{ title: `No access yet · ${brand.name}` }] }),
	ssr: false,
	component: NoAccess,
});

function NoAccess() {
	const [email] = useState(() => readFlow().email);
	return (
		<AuthLayout scene="gate">
			<AuthStatus label="No access yet" alert>
				<StatusIcon icon="cross-circle" tone="danger" />
				<Title>No access yet</Title>
				<Lead>
					{email ? <Badge variant="secondary">{email}</Badge> : "This account"}{" "}
					isn't part of an organization on {brand.name}. Ask your admin for an
					invitation, or sign in with another account.
				</Lead>
				<Link
					to="/sign-in"
					onClick={clearFlow}
					className={buttonVariants({ size: "lg" })}
				>
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
