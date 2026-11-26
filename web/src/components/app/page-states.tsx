import { type ErrorComponentProps, useRouter } from "@tanstack/react-router";
import { StatusMark } from "#/components/icons/status-mark.tsx";
import { Button } from "#/components/ui/button.tsx";
import { ApiError } from "#/lib/api/index.ts";

/** A page's data on its way. */
export function PageLoading() {
	return (
		<div className="grid flex-1 place-items-center p-6">
			<StatusMark mark="working" label="Loading" size={18} />
		</div>
	);
}

/** A page whose data wouldn't load, with a way to try again. */
export function PageError({ error }: Pick<ErrorComponentProps, "error">) {
	const router = useRouter();
	// Already on its way to /sign-in: nothing to say meanwhile.
	if (error instanceof ApiError && error.signedOut) return null;
	return (
		<div className="grid flex-1 place-items-center p-6 text-center">
			<div className="flex max-w-sm flex-col items-center gap-3">
				<h2 className="m-0 font-semibold text-[17px]">This didn't load</h2>
				<p role="alert" className="m-0 text-[13px] text-muted-foreground">
					{error instanceof ApiError
						? error.message
						: "Something went wrong. Try again."}
				</p>
				<Button onClick={() => router.invalidate()}>Try again</Button>
			</div>
		</div>
	);
}
