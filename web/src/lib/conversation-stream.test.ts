import { describe, expect, it } from "vitest";
import type { ConversationEvent } from "./api/index.ts";
import {
	ConversationStream,
	type EventSourceLike,
	listen,
	MAX_FAILURES,
	parseEvent,
	SETTLE_MS,
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
	const pending: { id: number; fn: () => void; ms: number }[] = [];
	let ids = 0;
	return {
		pending,
		timers: {
			set: (fn: () => void, ms: number) => {
				ids += 1;
				pending.push({ id: ids, fn, ms });
				return ids;
			},
			clear: (id: unknown) => {
				const at = pending.findIndex((t) => t.id === id);
				if (at >= 0) pending.splice(at, 1);
			},
		},
		next() {
			const t = pending.shift();
			t?.fn();
			return t?.ms;
		},
	};
}

const flush = () => new Promise((r) => setTimeout(r, 0));

function setup(checkSession?: () => Promise<boolean>) {
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
		checkSession,
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
		second.send(3, "tool", { name: "web.read_page", status: "running" });
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

	it("gives up after a few failures in a row, says so, and stops asking", async () => {
		const { sources, stream, clock: c } = setup();
		const lost: boolean[] = [];
		stream.subscribe(
			() => {},
			(l) => lost.push(l),
		);
		await flush();
		const waits: (number | undefined)[] = [];
		for (let i = 0; i < MAX_FAILURES; i++) {
			(sources.at(-1) as FakeSource).fail();
			waits.push(c.next());
			await flush();
		}
		expect(waits).toEqual([1_000, 2_000, 5_000, 10_000, 20_000]);
		(sources.at(-1) as FakeSource).fail();
		await flush();
		expect(c.pending).toHaveLength(0);
		expect(sources).toHaveLength(MAX_FAILURES + 1);
		expect(stream.gaveUp).toBe(true);
		expect(lost).toEqual([true]);
	});

	it("checks the session once when it gives up, and says nothing if it's gone", async () => {
		let checks = 0;
		const {
			sources,
			stream,
			clock: c,
		} = setup(async () => {
			checks += 1;
			return false; // signed out: call() is already on its way to /sign-in
		});
		const lost: boolean[] = [];
		stream.subscribe(
			() => {},
			(l) => lost.push(l),
		);
		await flush();
		for (let i = 0; i <= MAX_FAILURES; i++) {
			(sources.at(-1) as FakeSource).fail();
			c.next();
			await flush();
		}
		expect(checks).toBe(1);
		expect(lost).toEqual([]);
		expect(c.pending).toHaveLength(0);
	});

	it("closes once a turn has ended, and not on an earlier turn's done in a replay", async () => {
		const { sources, stream, clock: c } = setup();
		stream.subscribe(() => {});
		await flush();
		const s = sources[0] as FakeSource;
		s.open();
		// The replay: turn one ended, turn two carries on straight after.
		s.send(1, "message", { role: "user", text: "One" });
		s.send(2, "done", {});
		s.send(3, "message", { role: "user", text: "Two" });
		expect(c.pending).toHaveLength(0);
		expect(s.closed).toBe(false);

		s.send(4, "message", { role: "assistant", text: "Done" });
		s.send(5, "done", {});
		expect(c.next()).toBe(SETTLE_MS);
		expect(s.closed).toBe(true);
		expect(c.pending).toHaveLength(0);
	});

	it("closes when a turn ends in an error", async () => {
		const { sources, stream, clock: c } = setup();
		stream.subscribe(() => {});
		await flush();
		const s = sources[0] as FakeSource;
		s.send(1, "error", { code: "internal", message: "No" });
		c.next();
		expect(s.closed).toBe(true);
	});

	it("stays open for a reply sent just after a turn ended", async () => {
		const { sources, stream, seen, clock: c } = setup();
		stream.subscribe((e) => seen.push(e));
		await flush();
		const s = sources[0] as FakeSource;
		s.open();
		s.send(1, "message", { role: "user", text: "Hi" });
		s.send(2, "done", {});
		expect(c.pending.map((t) => t.ms)).toEqual([SETTLE_MS]);

		// The person replies within the 1.5 s: the pending close is dropped.
		stream.resume();
		expect(c.pending).toHaveLength(0);
		c.next();
		expect(s.closed).toBe(false);
		expect(sources).toHaveLength(1);

		s.send(3, "message", { role: "user", text: "And this?" });
		expect(seen.map((e) => e.id)).toEqual([1, 2, 3]);
	});

	it("closes again when a finished conversation is shown again and nothing is new", async () => {
		const { sources, stream, clock: c } = setup();
		stream.subscribe(() => {});
		await flush();
		const first = sources[0] as FakeSource;
		first.open();
		first.send(1, "message", { role: "user", text: "Hi" });
		first.send(2, "done", {});
		c.next();
		expect(first.closed).toBe(true);

		// The tab is shown again: the replay holds only what was already shown.
		stream.resume();
		await flush();
		const second = sources[1] as FakeSource;
		second.open();
		second.send(1, "message", { role: "user", text: "Hi" });
		second.send(2, "done", {});
		expect(c.next()).toBe(SETTLE_MS);
		expect(second.closed).toBe(true);
		expect(c.pending).toHaveLength(0);
	});

	it("stays open when something new arrives after a finished conversation reopens", async () => {
		const { sources, stream, clock: c } = setup();
		stream.subscribe(() => {});
		await flush();
		const first = sources[0] as FakeSource;
		first.send(1, "done", {});
		c.next();

		stream.resume();
		await flush();
		const second = sources[1] as FakeSource;
		second.open();
		second.send(1, "done", {});
		second.send(2, "message", { role: "user", text: "From another tab" });
		expect(c.pending).toHaveLength(0);
		expect(second.closed).toBe(false);
	});

	it("reopens on resume, showing only what's new", async () => {
		const { sources, stream, seen, clock: c } = setup();
		stream.subscribe((e) => seen.push(e));
		await flush();
		const first = sources[0] as FakeSource;
		first.send(1, "message", { role: "user", text: "Hi" });
		first.send(2, "done", {});
		c.next();
		expect(first.closed).toBe(true);

		stream.resume();
		await flush();
		const second = sources[1] as FakeSource;
		second.send(1, "message", { role: "user", text: "Hi" });
		second.send(2, "done", {});
		second.send(3, "message", { role: "user", text: "And this" });
		expect(seen.map((e) => e.id)).toEqual([1, 2, 3]);
	});

	it("pauses while the page is hidden and opens again when it's back", async () => {
		const sources: FakeSource[] = [];
		const page = listen("cnv_hidden", () => {}, {
			connect: () => {
				const s = new FakeSource();
				sources.push(s);
				return s;
			},
		});
		await flush();
		page.pause();
		expect((sources[0] as FakeSource).closed).toBe(true);
		page.resume();
		await flush();
		expect(sources).toHaveLength(2);
		expect((sources[1] as FakeSource).closed).toBe(false);
		// Resuming an open stream does nothing.
		page.resume();
		await flush();
		expect(sources).toHaveLength(2);
		page.stop();
	});

	it("can be resumed after it gave up", async () => {
		const { sources, stream, clock: c } = setup();
		const lost: boolean[] = [];
		stream.subscribe(
			() => {},
			(l) => lost.push(l),
		);
		await flush();
		for (let i = 0; i <= MAX_FAILURES; i++) {
			(sources.at(-1) as FakeSource).fail();
			c.next();
			await flush();
		}
		expect(stream.gaveUp).toBe(true);
		stream.resume();
		await flush();
		(sources.at(-1) as FakeSource).open();
		expect(lost).toEqual([true, false]);
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
		const a = listen("cnv_shared", () => {}, { connect });
		const b = listen("cnv_shared", () => {}, { connect });
		await flush();
		expect(opened).toBe(1);
		a.stop();
		b.stop();
		// Gone with its last listener: the next one opens afresh.
		const c = listen("cnv_shared", () => {}, { connect });
		await flush();
		expect(opened).toBe(2);
		c.stop();
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
