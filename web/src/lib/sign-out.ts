import { ApiError } from "#/lib/api/index.ts";

/**
 * Signs out on the core and leaves only once the session is gone: on success, or when the core
 * says it already was. Any other failure is reported and the person stays signed in.
 */
export async function signOut({
	request,
	leave,
	report,
}: {
	request: () => Promise<void>;
	leave: () => Promise<void> | void;
	report: (message: string) => void;
}): Promise<boolean> {
	try {
		await request();
	} catch (e) {
		if (!(e instanceof ApiError && e.signedOut)) {
			report(
				e instanceof ApiError
					? `Couldn't sign you out. ${e.message}`
					: "Couldn't sign you out. Try again.",
			);
			return false;
		}
	}
	await leave();
	return true;
}
