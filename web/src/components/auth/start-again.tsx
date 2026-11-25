import {
	Aside,
	AuthForm,
	AuthLayout,
	Lead,
	TextLink,
	Title,
} from "#/components/auth/auth-layout.tsx";

/** Shown when a step is opened without the sign-in that leads to it, or after it has expired. */
export function StartAgain() {
	return (
		<AuthLayout scene="gate">
			<AuthForm label="Start again">
				<Title>Start again</Title>
				<Lead>
					This step belongs to a sign-in that has finished or expired.
				</Lead>
				<Aside>
					<TextLink to="/sign-in">Back to sign in</TextLink>
				</Aside>
			</AuthForm>
		</AuthLayout>
	);
}
