import {
	type ApprovalState,
	type ConversationEvent,
	conversationEventsUrl,
	type ToolStatus,
} from "#/lib/api/index.ts";

/** The part of the browser's EventSource the stream uses, so tests and stand-ins can supply one. */
export type EventSourceLike = {
	/** 0 connecting, 1 open, 2 closed. */
	readyState: number;
	onopen: (() => void) | null;
	onerror: (() => void) | null;
	addEventListener(
		kind: string,
		fn: (e: { data: string; lastEventId: string }) => void,
	): void;
	close(): void;
};

export type Connect = (
	conversationId: string,
) => EventSourceLike | Promise<EventSourceLike>;

type Timers = {
	set: (fn: () => void, ms: number) => unknown;
	clear: (handle: unknown) => void;
};

const KINDS = [
	"message",
	"tool",
	"approval_wait",
	"approval_done",
	"error",
	"done",
] as const;

const TOOL_STATUSES = new Set<string>([
	"requested",
	"running",
	"completed",
	"denied",
	"failed",
]);
const APPROVAL_STATES = new Set<string>([
	"pending",
	"approved",
	"denied",
	"used",
	"expired",
]);

const str = (v: unknown): v is string => typeof v === "string";

/** One event from the core's stream, or null when it isn't one the web app knows. */
export function parseEvent(
	kind: string,
	id: number,
	raw: string,
): ConversationEvent | null {
	let d: Record<string, unknown>;
	try {
		d = JSON.parse(raw);
	} catch {
		return null;
	}
	if (typeof d !== "object" || d === null) return null;
	switch (kind) {
		case "message":
			return (d.role === "user" || d.role === "assistant") && str(d.text)
				? { id, kind, role: d.role, text: d.text }
				: null;
		case "tool":
			return str(d.name) && str(d.status) && TOOL_STATUSES.has(d.status)
				? { id, kind, name: d.name, status: d.status as ToolStatus }
				: null;
		case "approval_wait": {
			const a = d.approval as { id?: unknown } | undefined;
			return a && str(a.id)
				? { id, kind, approval: d.approval as never }
				: null;
		}
		case "approval_done":
			return str(d.approval_id) && str(d.state) && APPROVAL_STATES.has(d.state)
				? {
						id,
						kind,
						approval_id: d.approval_id,
						state: d.state as ApprovalState,
					}
				: null;
		case "error":
			return str(d.code) && str(d.message)
				? { id, kind, code: d.code, message: d.message }
				: null;
		case "done":
			return { id, kind };
		default:
			return null;
	}
}

// After the core refuses a stream (429 when too many are open) or a reconnect fails, wait longer
// each time before trying again.
const BACKOFF_MS = [1_000, 2_000, 5_000, 10_000, 20_000, 30_000];

/**
 * One live stream of a conversation's events, shared by everyone listening to it.
 *
 * A stream the core ends after five minutes is reopened by the browser itself, which sends
 * Last-Event-ID so the core carries on after it. A stream that fails outright (a 429, or a
 * reconnect the browser gives up on) is reopened here after a pause; a fresh EventSource can't
 * send Last-Event-ID, so the core replays from the start, and events already seen are dropped by id.
 */
export class ConversationStream {
	private source: EventSourceLike | null = null;
	private listeners = new Set<(e: ConversationEvent) => void>();
	private lastId = 0;
	private failures = 0;
	private retry: unknown = null;
	private closed = true;

	constructor(
		readonly conversationId: string,
		private readonly connect: Connect,
		private readonly timers: Timers = {
			set: (fn, ms) => setTimeout(fn, ms),
			clear: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
		},
	) {}

	/** The id of the last event passed on; what Last-Event-ID would say. */
	get lastEventId() {
		return this.lastId;
	}

	get listening() {
		return this.listeners.size;
	}

	subscribe(fn: (e: ConversationEvent) => void): () => void {
		this.listeners.add(fn);
		if (this.closed) {
			this.closed = false;
			this.open();
		}
		return () => {
			this.listeners.delete(fn);
			if (this.listeners.size === 0) this.close();
		};
	}

	close() {
		this.closed = true;
		if (this.retry !== null) this.timers.clear(this.retry);
		this.retry = null;
		this.source?.close();
		this.source = null;
	}

	private async open() {
		const source = await this.connect(this.conversationId);
		if (this.closed) {
			source.close();
			return;
		}
		this.source = source;
		for (const kind of KINDS) {
			source.addEventListener(kind, (e) => this.receive(kind, e));
		}
		source.onopen = () => {
			this.failures = 0;
		};
		source.onerror = () => {
			// Still connecting: the browser is reconnecting on its own, with Last-Event-ID.
			if (source.readyState !== 2 || this.source !== source) return;
			source.close();
			this.source = null;
			const wait =
				BACKOFF_MS[Math.min(this.failures, BACKOFF_MS.length - 1)] ?? 30_000;
			this.failures += 1;
			this.retry = this.timers.set(() => {
				this.retry = null;
				if (!this.closed) this.open();
			}, wait);
		};
	}

	private receive(kind: string, e: { data: string; lastEventId: string }) {
		const id = Number(e.lastEventId);
		// An id at or before the last one is a replay of something already shown.
		if (!Number.isSafeInteger(id) || id <= this.lastId) return;
		const event = parseEvent(kind, id, e.data);
		if (!event) return;
		this.lastId = id;
		for (const fn of this.listeners) fn(event);
	}
}

/** A source that never opens, for a build with no core to reach. */
const nowhere: EventSourceLike = {
	readyState: 2,
	onopen: null,
	onerror: null,
	addEventListener() {},
	close() {},
};

/** The browser's EventSource with the session cookie; stand-ins in dev, where there's no core. */
const connectToCore: Connect = async (id) => {
	const url = conversationEventsUrl(id);
	// The real EventSource has wider signatures than the slice of it the stream uses.
	if (url)
		return new EventSource(url, {
			withCredentials: true,
		}) as unknown as EventSourceLike;
	// Inside the DEV branch, so a production bundle never carries the stand-ins.
	if (import.meta.env.DEV) {
		const { standInEvents } = await import("#/lib/api/standin.ts");
		return standInEvents(id);
	}
	return nowhere;
};

// At most one stream per conversation: the core allows five per person.
const streams = new Map<string, ConversationStream>();

/** Listens to a conversation's events; the stream closes when its last listener goes. */
export function listen(
	conversationId: string,
	fn: (e: ConversationEvent) => void,
	connect: Connect = connectToCore,
): () => void {
	let stream = streams.get(conversationId);
	if (!stream) {
		stream = new ConversationStream(conversationId, connect);
		streams.set(conversationId, stream);
	}
	const s = stream;
	const stop = s.subscribe(fn);
	return () => {
		stop();
		if (s.listening === 0) streams.delete(conversationId);
	};
}
