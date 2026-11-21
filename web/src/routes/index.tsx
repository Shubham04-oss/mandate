import { createFileRoute, redirect } from "@tanstack/react-router";

// ponytail: until the core says whether someone is signed in, the app starts at sign-in.
export const Route = createFileRoute("/")({
	beforeLoad: () => {
		throw redirect({ to: "/sign-in" });
	},
});
