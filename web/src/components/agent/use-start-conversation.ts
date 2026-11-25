import { useNavigate, useRouter } from "@tanstack/react-router";
import { toast } from "sonner";
import { ApiError, api } from "#/lib/api/index.ts";

/** Starts a conversation with your agent and opens it; resolves false when it couldn't start. */
export function useStartConversation() {
	const navigate = useNavigate();
	const router = useRouter();
	return async (message: string): Promise<boolean> => {
		try {
			const c = await api.conversations.start(message);
			await navigate({
				to: "/agent/$conversationId",
				params: { conversationId: c.id },
			});
			return true;
		} catch (e) {
			toast(
				e instanceof ApiError ? e.message : "Something went wrong. Try again.",
			);
			// A start that failed can still leave a failed conversation in the list.
			void router.invalidate();
			return false;
		}
	};
}
