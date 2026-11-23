import { describe, expect, it } from "vitest";
import type { ConversationEvent } from "./api/index.ts";
import {
	ConversationStream,
	type EventSourceLike,
	listen,
	parseEvent,
} from "./conversation-stream.ts";

/** A browser EventSource the test drives: what the core sends, and how the connection ends. */
class FakeSource implements EventSourceLike {
	readyState = 0;
	onopen: (() => void) | null = null;
	onerror: (() => void) | null = null;
	closed = false;
	private handlers = new Map<
		string,
		((e: { data: string; lastEventId: string }) => void)[]
	>();

	addEventListener(
		kind: string,
		fn: (e: { data: string; lastEventId: string }) => void,
	) {
		this.handlers.set(kind, [...(this.handlers.get(kind) ?? []), fn]);
	}
	close() {
		this.closed = true;
		this.readyState = 2;
	}
	open() {
		this.readyState = 1;
		this.onopen?.();
	}
	send(id: number, kind: string, data: unknown) {
		for (const fn of this.handlers.get(kind) ?? [])
			fn({ data: JSON.stringify(data), lastEventId: String(id) });
	}
	/** The core refused the stream (a 429) or the browser gave up reconnecting. */
	fail() {
		this.readyState = 2;
		this.onerror?.();
	}
	/** The core ended the stream after five minutes; the browser reconnects by itself. */
	drop() {
		this.readyState = 0;
		this.onerror?.();
	}
}

/** Timers the test runs by hand. */
function clock() {
	const pending: { fn: () => void; ms: number }[] = [];
	return {
		pending,
		timers: {
			set: (fn: () => void, ms: number) => {
				pending.push({ fn, ms });
				return pending.length;
			},
			clear: () => {},
		},
		next() {
			const t = pending.shift();
			t?.fn();
			return t?.ms;
		},
	};
}

const flush = () => new Promise((r) => setTimeout(r, 0));

function setup() {
	const sources: FakeSource[] = [];
	const c = clock();
	const stream = new ConversationStream(
		"cnv_1",
		() => {
			const s = new FakeSource();
			sources.push(s);
			return s;
		},
		c.timers,
	);
	const seen: ConversationEvent[] = [];
	return { sources, clock: c, stream, seen };
}

describe("the conversation stream", () => {
	it("passes events on once, in order, with their ids", async () => {
		const { sources, stream, seen } = setup();
		stream.subscribe((e) => seen.push(e));
		await flush();
		const s = sources[0] as FakeSource;
		s.open();
		s.send(1, "message", { role: "user", text: "Hi" });
		s.send(2, "tool", { name: "web.read_page", status: "running" });
		expect(seen.map((e) => [e.id, e.kind])).toEqual([
			[1, "message"],
			[2, "tool"],
		]);
		expect(stream.lastEventId).toBe(2);
	});

	it("lets the browser reconnect by itself when the core ends the stream", async () => {
		const { sources, stream, clock: c } = setup();
		stream.subscribe(() => {});
		await flush();
		(sources[0] as FakeSource).drop();
		// No new connection and no timer: the browser reconnects with Last-Event-ID itself.
		expect(sources).toHaveLength(1);
		expect(c.pending).toHaveLength(0);
	});

	it("reconnects after a refusal, backing off, and drops what it already showed", async () => {
		const { sources, stream, seen, clock: c } = setup();
		stream.subscribe((e) => seen.push(e));
		await flush();
		const first = sources[0] as FakeSource;
		first.open();
		first.send(1, "message", { role: "user", text: "Hi" });
		first.send(2, "message", { role: "assistant", text: "Hello" });

		first.fail();
		expect(first.closed).toBe(true);
		expect(c.next()).toBe(1_000);
		await flush();

		// A fresh EventSource can't send Last-Event-ID, so the core replays from the start.
		const second = sources[1] as FakeSource;
		second.send(1, "message", { role: "user", text: "Hi" });
		second.send(2, "message", { role: "assistant", text: "Hello" });
		second.send(3, "done", {});
		expect(seen.map((e) => e.id)).toEqual([1, 2, 3]);

		// Refused again before it opened: the pause grows.
		second.fail();
		expect(c.next()).toBe(2_000);
	});

	it("starts the pause over once a connection opens", async () => {
		const { sources, stream, clock: c } = setup();
		stream.subscribe(() => {});
		await flush();
		(sources[0] as FakeSource).fail();
		c.next();
		await flush();
		const second = sources[1] as FakeSource;
		second.open();
		second.fail();
		expect(c.next()).toBe(1_000);
	});

	it("ignores events it doesn't understand", async () => {
		const { sources, stream, seen } = setup();
		stream.subscribe((e) => seen.push(e));
		await flush();
		const s = sources[0] as FakeSource;
		s.send(1, "tool", { name: "web.read_page", status: "exploded" });
		s.send(2, "message", { role: "system", text: "x" });
		s.send(3, "message", { role: "assistant", text: "Fine" });
		expect(seen.map((e) => e.id)).toEqual([3]);
	});

	it("closes when the last listener leaves, and never reconnects after", async () => {
		const { sources, stream, clock: c } = setup();
		const stop = stream.subscribe(() => {});
		await flush();
		stop();
		expect((sources[0] as FakeSource).closed).toBe(true);
		expect(c.pending).toHaveLength(0);
	});
});

describe("listen", () => {
	it("opens one stream per conversation however many listen", async () => {
		let opened = 0;
		const connect = () => {
			opened += 1;
			return new FakeSource();
		};
		const a = listen("cnv_shared", () => {}, connect);
		const b = listen("cnv_shared", () => {}, connect);
		await flush();
		expect(opened).toBe(1);
		a();
		b();
		// Gone with its last listener: the next one opens afresh.
		const c = listen("cnv_shared", () => {}, connect);
		await flush();
		expect(opened).toBe(2);
		c();
	});
});

describe("parseEvent", () => {
	it("reads each kind the core sends", () => {
		expect(parseEvent("done", 4, "{}")).toEqual({ id: 4, kind: "done" });
		expect(
			parseEvent(
				"approval_done",
				5,
				JSON.stringify({ approval_id: "a1", state: "approved" }),
			),
		).toEqual({
			id: 5,
			kind: "approval_done",
			approval_id: "a1",
			state: "approved",
		});
		expect(
			parseEvent(
				"error",
				6,
				JSON.stringify({ code: "internal", message: "No" }),
			),
		).toEqual({ id: 6, kind: "error", code: "internal", message: "No" });
		expect(parseEvent("message", 7, "not json")).toBeNull();
	});
});
