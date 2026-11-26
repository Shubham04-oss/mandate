/** A run of a message: plain text, or a web address that can be a link. */
export type Segment =
	| { kind: "text"; text: string }
	| { kind: "link"; href: string; text: string };

// http(s) only: nothing else in a message ever becomes clickable.
const URL_PATTERN = /https?:\/\/[^\s<>"']+/g;
// Sentence punctuation that ends a URL in prose rather than belonging to it.
const TRAILING = /[.,;:!?)\]]+$/;

/** Splits plain text into text and links. Messages are never rendered as HTML. */
export function linkify(text: string): Segment[] {
	const out: Segment[] = [];
	let last = 0;
	for (const m of text.matchAll(URL_PATTERN)) {
		const start = m.index ?? 0;
		const raw = m[0];
		const href = raw.replace(TRAILING, "");
		if (start > last) out.push({ kind: "text", text: text.slice(last, start) });
		let valid = false;
		try {
			const u = new URL(href);
			valid = u.protocol === "http:" || u.protocol === "https:";
		} catch {
			valid = false;
		}
		out.push(
			valid ? { kind: "link", href, text: href } : { kind: "text", text: href },
		);
		last = start + href.length;
	}
	if (last < text.length) out.push({ kind: "text", text: text.slice(last) });
	return out;
}
