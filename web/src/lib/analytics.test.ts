import type { CaptureResult } from "posthog-js";
import { describe, expect, it, vi } from "vitest";
import {
	cleanProps,
	config,
	forceTestError,
	lengthBucket,
	scrubEvent,
	waitedMs,
} from "./analytics.ts";

const origin = "https://app.example.com";
// A stand-in for the router: conversation pages share one pattern, unknown paths have none.
const pattern = (pathname: string) =>
	pathname.startsWith("/agent/")
		? "/agent/$conversationId"
		: ["/", "/sign-in", "/reset-password"].includes(pathname)
			? pathname
			: "/not-found";

describe("named events", () => {
	it("passes what an event declares", () => {
		expect(
			cleanProps("approval_answered", { outcome: "denied", wait_ms: 1500 }),
		).toEqual({ outcome: "denied", wait_ms: 1500 });
		expect(
			cleanProps("message_sent", { length: "21-100", first: true }),
		).toEqual({ length: "21-100", first: true });
		expect(cleanProps("approval_answered", { wait_ms: undefined })).toEqual({});
		expect(cleanProps("conversation_stopped", {})).toEqual({});
	});

	it("drops free text, wherever it is put", () => {
		expect(
			cleanProps("approval_answered", { outcome: "my password is hunter2" }),
		).toBe(null);
		expect(cleanProps("message_sent", { length: "Q3 revenue figures" })).toBe(
			null,
		);
		expect(cleanProps("message_sent", { text: "hello" })).toBe(null);
		expect(
			cleanProps("approval_answered", { outcome: "denied", note: "x" }),
		).toBe(null);
		expect(cleanProps("approval_shown", { can_decide: "yes" })).toBe(null);
		expect(cleanProps("approval_answered", { wait_ms: "1500" })).toBe(null);
		expect(cleanProps("approval_answered", { wait_ms: Number.NaN })).toBe(null);
		expect(cleanProps("approval_answered", { outcome: { a: 1 } })).toBe(null);
	});

	it("knows only its own events; the core's are not among them", () => {
		for (const event of [
			"conversation_started",
			"approval_decided",
			"grant_created",
			"signed_up",
			"signed_in",
			"org_chosen",
			"settings_changed",
			"$pageview",
			"toString",
		])
			expect(cleanProps(event, {}), event).toBe(null);
	});

	it("buckets a message's length", () => {
		expect(lengthBucket("hi")).toBe("1-20");
		expect(lengthBucket("x".repeat(20))).toBe("1-20");
		expect(lengthBucket("x".repeat(21))).toBe("21-100");
		expect(lengthBucket("x".repeat(500))).toBe("101-500");
		expect(lengthBucket("x".repeat(501))).toBe("501+");
	});

	it("measures a wait, and gives none for a time it can't read", () => {
		const now = Date.parse("2026-11-27T10:00:05Z");
		expect(waitedMs("2026-11-27T10:00:00Z", now)).toBe(5000);
		expect(waitedMs("2026-11-27T10:00:09Z", now)).toBe(0);
		expect(waitedMs("not a time", now)).toBeUndefined();
	});
});

const event = (
	properties: Record<string, unknown>,
	more: Partial<CaptureResult> = {},
) =>
	({
		uuid: "u",
		event: "$pageview",
		properties,
		...more,
	}) as CaptureResult;

describe("scrubbing outgoing events", () => {
	it("turns every address into its route pattern", () => {
		const out = scrubEvent(
			event(
				{
					$current_url: `${origin}/agent/c-123?code=SECRET#frag`,
					$pathname: "/agent/c-123",
					$session_entry_url: `${origin}/reset-password?token=SECRET`,
					$session_entry_pathname: "/reset-password",
					$prev_pageview_pathname: "/sign-in",
					$host: "app.example.com",
					$browser: "Firefox",
				},
				{
					$set_once: {
						$initial_current_url: `${origin}/sign-in?error=SECRET`,
						$initial_pathname: "/sign-in",
					},
				},
			),
			origin,
			pattern,
		);
		expect(out.properties).toEqual({
			$current_url: `${origin}/agent/$conversationId`,
			$pathname: "/agent/$conversationId",
			$session_entry_url: `${origin}/reset-password`,
			$session_entry_pathname: "/reset-password",
			$prev_pageview_pathname: "/sign-in",
			$host: "app.example.com",
			$browser: "Firefox",
		});
		expect(out.$set_once).toEqual({
			$initial_current_url: `${origin}/sign-in`,
			$initial_pathname: "/sign-in",
		});
	});

	it("drops referrers, titles and addresses on other sites", () => {
		const out = scrubEvent(
			event(
				{
					$referrer: "https://idp.example.com/cb?code=SECRET",
					$referring_domain: "idp.example.com",
					$session_entry_referrer: "https://idp.example.com/cb?code=SECRET",
					title: "Q3 revenue figures",
					$current_url: "https://elsewhere.example/x?code=SECRET",
					$set: { $referrer: "https://idp.example.com/?code=SECRET" },
				},
				{ $set_once: { $initial_referrer: "https://idp.example.com/" } },
			),
			origin,
			pattern,
		);
		expect(out.properties).toEqual({ $set: {} });
		expect(out.$set_once).toEqual({});
	});

	it("keeps an exception's type and stack, never its message", () => {
		const out = scrubEvent(
			event({
				$exception_list: [
					{
						type: "SyntaxError",
						value: "Unexpected token in SECRET",
						mechanism: { handled: false, synthetic: false },
						stacktrace: {
							type: "raw",
							frames: [
								{
									filename: `${origin}/assets/main-abc.js?v=1`,
									function: "send",
									lineno: 3,
									colno: 7,
									in_app: true,
									context_line: "SECRET",
								},
								{
									filename: `${origin}/reset-password?token=SECRET#x`,
									abs_path: `${origin}/reset-password?token=SECRET`,
									lineno: 1,
								},
							],
						},
					},
				],
				$exception_values: ["SECRET"],
				$exception_message: "SECRET",
				$exception_steps: [{ $message: "SECRET" }],
				$exception_level: "error",
			}),
			origin,
			pattern,
		);
		expect(JSON.stringify(out)).not.toContain("SECRET");
		expect(out.properties.$exception_level).toBe("error");
		expect(out.properties.$exception_list).toEqual([
			{
				type: "SyntaxError",
				mechanism: { handled: false, synthetic: false },
				stacktrace: {
					type: "raw",
					frames: [
						{
							filename: `${origin}/assets/main-abc.js`,
							function: "send",
							lineno: 3,
							colno: 7,
							in_app: true,
						},
						{
							filename: `${origin}/reset-password`,
							abs_path: `${origin}/reset-password`,
							lineno: 1,
						},
					],
				},
			},
		]);
	});

	it("scrubs the addresses inside a web vitals event", () => {
		const out = scrubEvent(
			event({
				$current_url: `${origin}/agent/c-1?code=SECRET`,
				$web_vitals_LCP_value: 1234,
				$web_vitals_LCP_event: {
					name: "LCP",
					value: 1234,
					$current_url: `${origin}/agent/c-1?code=SECRET`,
					navigationURL: `${origin}/agent/c-1?code=SECRET`,
					attribution: { url: `${origin}/img?code=SECRET`, target: "img" },
				},
			}),
			origin,
			pattern,
		);
		expect(out.properties.$web_vitals_LCP_event).toEqual({
			name: "LCP",
			value: 1234,
			$current_url: `${origin}/agent/$conversationId`,
			navigationURL: `${origin}/agent/$conversationId`,
		});
		expect(out.properties.$web_vitals_LCP_value).toBe(1234);
	});
});

describe("the PostHog config", () => {
	it("loads nothing external, records nothing on its own and keeps nothing on the device", () => {
		const c = config(null);
		expect(c).toMatchObject({
			api_host: "/ingest",
			ui_host: "https://eu.posthog.com",
			disable_external_dependency_loading: true,
			advanced_disable_flags: true,
			persistence: "memory",
			person_profiles: "identified_only",
			autocapture: false,
			capture_pageview: false,
			disable_session_recording: true,
			disable_surveys: true,
			save_referrer: false,
			save_campaign_params: false,
		});
		expect(c.bootstrap).toEqual({});
	});

	it("starts a signed-in page as the member, so a reload merges nothing", () => {
		expect(config({ member: "user_01", org: "org_01" }).bootstrap).toEqual({
			distinctID: "user_01",
			isIdentifiedID: true,
		});
	});
});

describe("the forced test error", () => {
	it("throws an uncaught Error on the next tick, not in the caller", () => {
		vi.useFakeTimers();
		try {
			expect(() => forceTestError()).not.toThrow();
			expect(() => vi.runAllTimers()).toThrow(new Error("forced test error"));
		} finally {
			vi.useRealTimers();
		}
	});
});
