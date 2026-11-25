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
// each time before trying again, and stop after this many failures in a row (about 38 s).
const BACKOFF_MS = [1_000, 2_000, 5_000, 10_000, 20_000];
export const MAX_FAILURES = BACKOFF_MS.length;

/** Told when the stream loses the core after its last retry, and when it has it again. */
export type OnLost = (lost: boolean) => void;

// A turn has ended once `done` or `error` is followed by this much quiet: a fresh stream replays
// every earlier turn's `done` on its way to the latest events, which arrive straight after.
export const SETTLE_MS = 1_500;

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
	private lostListeners = new Set<OnLost>();
	private lost = false;
	private lastId = 0;
	private failures = 0;
	private retry: unknown = null;
	private settle: unknown = null;
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

	/** True once the stream has given up on the core; it stays closed until reopened. */
	get gaveUp() {
		return this.lost;
	}

	subscribe(fn: (e: ConversationEvent) => void, onLost?: OnLost): () => void {
		this.listeners.add(fn);
		if (onLost) this.lostListeners.add(onLost);
		if (this.closed) {
			this.closed = false;
			this.open();
		}
		return () => {
			this.listeners.delete(fn);
			if (onLost) this.lostListeners.delete(onLost);
			if (this.listeners.size === 0) this.close();
		};
	}

	private setLost(lost: boolean) {
		if (this.lost === lost) return;
		this.lost = lost;
		for (const fn of this.lostListeners) fn(lost);
	}

	/** Closes the connection; listeners stay, and resume() opens it again. */
	close() {
		this.closed = true;
		if (this.retry !== null) this.timers.clear(this.retry);
		if (this.settle !== null) this.timers.clear(this.settle);
		this.retry = null;
		this.settle = null;
		this.source?.close();
		this.source = null;
	}

	/** Opens the stream again after a turn ended, the page was hidden, or it gave up. */
	resume() {
		if (this.listeners.size === 0) return;
		// Still open but about to close because the last turn ended: a send means a new turn, so keep it.
		if (this.settle !== null) this.timers.clear(this.settle);
		this.settle = null;
		if (!this.closed) return;
		this.closed = false;
		this.failures = 0;
		this.open();
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
			this.setLost(false);
		};
		source.onerror = () => {
			// Still connecting: the browser is reconnecting on its own, with Last-Event-ID.
			if (source.readyState !== 2 || this.source !== source) return;
			source.close();
			this.source = null;
			const wait = BACKOFF_MS[this.failures];
			this.failures += 1;
			// Out of retries: stay closed instead of asking the core every 30 s forever.
			if (wait === undefined) {
				this.closed = true;
				this.setLost(true);
				return;
			}
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
		// Close once the turn has ended, so an idle conversation holds none of the core's streams.
		if (this.settle !== null) this.timers.clear(this.settle);
		this.settle = null;
		if (event.kind === "done" || event.kind === "error")
			this.settle = this.timers.set(() => {
				this.settle = null;
				this.close();
			}, SETTLE_MS);
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

export type Listening = {
	/** Stops listening; the stream closes when its last listener goes. */
	stop(): void;
	/** Closes the connection for now, for a page that is hidden. */
	pause(): void;
	/** Opens it again: the page is visible, or the person sent something. */
	resume(): void;
};

/** Listens to a conversation's events. */
export function listen(
	conversationId: string,
	fn: (e: ConversationEvent) => void,
	{
		connect = connectToCore,
		onLost,
	}: { connect?: Connect; onLost?: OnLost } = {},
): Listening {
	let stream = streams.get(conversationId);
	if (!stream) {
		stream = new ConversationStream(conversationId, connect);
		streams.set(conversationId, stream);
	}
	const s = stream;
	const stop = s.subscribe(fn, onLost);
	return {
		stop() {
			stop();
			if (s.listening === 0) streams.delete(conversationId);
		},
		pause: () => s.close(),
		resume: () => s.resume(),
	};
}
