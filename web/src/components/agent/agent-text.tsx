import { Fragment } from "react";
import { linkify } from "#/lib/linkify.ts";

/**
 * Your agent's words as plain text: paragraphs at blank lines, web addresses as links that open
 * in a new tab. Never HTML, whatever the message holds.
 */
export function AgentText({ text }: { text: string }) {
	// A message never changes once shown, so its paragraphs and runs are keyed by position.
	const paragraphs = text.split(/\n{2,}/).map((p, i) => ({
		key: `p${i}`,
		runs: linkify(p).map((run, j) => ({ ...run, key: `r${j}` })),
	}));
	return (
		<div className="flex flex-col gap-3">
			{paragraphs.map((p) => (
				<p
					key={p.key}
					className="m-0 whitespace-pre-wrap text-[15px] leading-[1.72]"
				>
					{p.runs.map((run) =>
						run.kind === "link" ? (
							<a
								key={run.key}
								href={run.href}
								target="_blank"
								rel="noopener noreferrer nofollow"
								className="break-all text-foreground underline decoration-border-strong underline-offset-2 hover:decoration-foreground"
							>
								{run.text}
							</a>
						) : (
							<Fragment key={run.key}>{run.text}</Fragment>
						),
					)}
				</p>
			))}
		</div>
	);
}
