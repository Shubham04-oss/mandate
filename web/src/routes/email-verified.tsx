import { createFileRoute, Link } from "@tanstack/react-router";
import {
	AuthLayout,
	AuthStatus,
	Lead,
	StatusIcon,
	Title,
} from "#/components/auth/auth-layout.tsx";
import { Icon } from "#/components/icons/icon.tsx";
import { buttonVariants } from "#/components/ui/button.tsx";
import { brand } from "#/lib/brand.ts";

export const Route = createFileRoute("/email-verified")({
	head: () => ({ meta: [{ title: `Email verified · ${brand.name}` }] }),
	component: EmailVerified,
});

function EmailVerified() {
	return (
		<AuthLayout scene="bay">
			<AuthStatus label="Email verified">
				<StatusIcon icon="check-circle" tone="ok" />
				<Title>Email verified</Title>
				<Lead>Your account is ready.</Lead>
				{/* ponytail: goes to sign-in until the app has a home to land on */}
				<Link to="/sign-in" className={buttonVariants({ size: "lg" })}>
					Continue <Icon name="arrow-right" />
				</Link>
			</AuthStatus>
		</AuthLayout>
	);
}
