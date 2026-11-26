import { Link } from "@tanstack/react-router";
import { cn } from "cn";
import type { ReactNode } from "react";
import { StatusMark } from "#/components/icons/status-mark.tsx";
import {
	Item,
	ItemActions,
	ItemContent,
	ItemDescription,
	ItemMedia,
	ItemTitle,
} from "#/components/ui/item.tsx";

export type Row = {
	id: string;
	kind: string;
	title: string;
	detail: string;
	when: string;
	/** A conversation to open, when the row is one. */
	conversationId?: string;
};

// Bare status marks, nothing behind them: how each row stands, in its own colour.
const marks: Record<string, ReactNode> = {
	working: <StatusMark mark="working" size={18} />,
	waiting: <StatusMark mark="needs-you" size={18} />,
	done: <StatusMark mark="done" size={18} />,
	approved: <StatusMark mark="done" size={18} />,
	refused: <StatusMark mark="failed" size={18} />,
};

/** One line of what happened: a link when it opens a conversation, otherwise just a row. */
export function RowItem({ row }: { row: Row }) {
	return (
		<Item
			size="xs"
			render={
				row.conversationId ? (
					<Link
						to="/agent/$conversationId"
						params={{ conversationId: row.conversationId }}
					/>
				) : undefined
			}
			// rows cut off long titles; the whole line is there on hover
			title={`${row.title}\n${row.detail}`}
			className={cn(
				"min-h-12 rounded-lg px-2.5 py-1.5 no-underline",
				row.conversationId && "hover:bg-hover",
			)}
		>
			<ItemMedia>{marks[row.kind]}</ItemMedia>
			<ItemContent className="min-w-0 gap-0">
				<ItemTitle className="block w-full truncate font-semibold text-[12.5px]">
					{row.title}
				</ItemTitle>
				<ItemDescription className="truncate text-[11.5px]">
					{row.detail}
				</ItemDescription>
			</ItemContent>
			<ItemActions className="text-[11.5px] text-muted-foreground tabular-nums">
				{row.when}
			</ItemActions>
		</Item>
	);
}
