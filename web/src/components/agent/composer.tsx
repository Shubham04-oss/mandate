import { type FormEvent, useState } from "react";
import { Icon } from "#/components/icons/icon.tsx";
import { Button } from "#/components/ui/button.tsx";
import { Textarea } from "#/components/ui/textarea.tsx";

/** Whether the composer's button stops your agent or sends: Stop only while it works and nothing is written. */
export function composerAction(
	working: boolean,
	text: string,
	canStop: boolean,
): "stop" | "send" {
	return working && !text.trim() && canStop ? "stop" : "send";
}

/**
 * Where you write to your agent. Enter sends, Shift+Enter starts a new line. While your agent
 * works and nothing is written, the button stops it instead.
 */
export function Composer({
	label,
	placeholder,
	onSend,
	working = false,
	onStop,
	autoFocus,
}: {
	label: string;
	placeholder: string;
	/** Resolves false when sending failed, so the words stay. */
	onSend: (text: string) => Promise<boolean>;
	working?: boolean;
	onStop?: () => void;
	autoFocus?: boolean;
}) {
	const [text, setText] = useState("");
	const [sending, setSending] = useState(false);
	const stops = composerAction(working, text, onStop !== undefined) === "stop";

	const send = async () => {
		const words = text.trim();
		if (!words || sending) return;
		setSending(true);
		const sent = await onSend(words);
		setSending(false);
		if (sent) setText("");
	};

	const submit = (e: FormEvent) => {
		e.preventDefault();
		if (stops) onStop?.();
		else void send();
	};

	return (
		<form
			aria-label={label}
			onSubmit={submit}
			className="flex items-end gap-2.5 rounded-[20px] bg-raised py-2.5 pr-2.5 pl-[18px] shadow-[inset_0_0_0_1px_var(--border-quiet),0_10px_30px_var(--shade-66)]"
		>
			<Textarea
				aria-label={label}
				placeholder={placeholder}
				value={text}
				autoFocus={autoFocus}
				rows={1}
				onChange={(e) => setText(e.target.value)}
				onKeyDown={(e) => {
					if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
						e.preventDefault();
						if (!stops) void send();
					}
				}}
				className="max-h-48 min-h-0 flex-1 resize-none rounded-none border-0 bg-transparent px-0 py-1.5 text-[14.5px] leading-normal shadow-none focus-visible:ring-0 md:text-[14.5px] dark:bg-transparent"
			/>
			<Button
				type="submit"
				size="icon"
				aria-label={stops ? "Stop" : "Send"}
				disabled={!stops && (!text.trim() || sending)}
				className="size-[34px] flex-none rounded-full"
			>
				{stops ? (
					<span
						aria-hidden="true"
						className="size-3 rounded-[3px] bg-current"
					/>
				) : (
					<Icon name="send" size={16} />
				)}
			</Button>
		</form>
	);
}
