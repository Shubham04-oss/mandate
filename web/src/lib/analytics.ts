import type { CaptureResult, PostHog, PostHogConfig } from "posthog-js";

/**
 * Product analytics (PostHog, EU), privacy-first. VITE_POSTHOG_KEY is PostHog's public project key,
 * fixed at build time like VITE_MANDATE_API_URL; without it nothing here loads or sends anything.
 *
 * What leaves the browser: our event names with enum, boolean and number properties, the member and
 * tenant ids the core returns (never names, emails or org names), route patterns instead of
 * addresses, and error types and stacks without their messages. Events go to /ingest on this origin
 * (src/lib/ingest.ts), which forwards them without cookies or the person's IP.
 */
export const posthogKey = (import.meta.env.VITE_POSTHOG_KEY ?? "").trim();

// ── Named events ─────────────────────────────────────────────────────────────────────────────────
// Only what the browser alone sees. The core sends sign-ups, sign-ins, org choices, settings,
// conversations started, approvals decided and grants created from its outbox, so the browser
// never does (they would count twice).

export const lengthBuckets = ["1-20", "21-100", "101-500", "501+"] as const;
export type LengthBucket = (typeof lengthBuckets)[number];

/** A message's length as a bucket, so the count can't narrow down what was written. */
export function lengthBucket(text: string): LengthBucket {
	const n = text.length;
	return n <= 20 ? "1-20" : n <= 100 ? "21-100" : n <= 500 ? "101-500" : "501+";
}

/** Milliseconds since an RFC 3339 time, or undefined for a time that doesn't parse. */
export function waitedMs(since: string, now = Date.now()): number | undefined {
	const waited = now - Date.parse(since);
	return Number.isFinite(waited) ? Math.max(0, waited) : undefined;
}

type Field = readonly string[] | "boolean" | "number";

/** Every event the web app sends and what each may carry: closed lists, booleans and numbers only. */
const events = {
	message_sent: { length: lengthBuckets, first: "boolean" },
	conversation_stopped: {},
	approval_shown: { can_decide: "boolean" },
	approval_answered: { outcome: ["approved", "denied"], wait_ms: "number" },
} as const satisfies Record<string, Record<string, Field>>;

export type EventName = keyof typeof events;
type Value<F> = F extends "boolean"
	? boolean
	: F extends "number"
		? number
		: F extends readonly (infer U)[]
			? U
			: never;
export type EventProps<E extends EventName> = {
	[K in keyof (typeof events)[E]]?: Value<(typeof events)[E][K]>;
};

/**
 * The props as sent, or null when anything isn't what the event declares: an unknown event or key,
 * a string outside its list, a number that isn't finite. Free text can't pass any of these.
 */
export function cleanProps(
	event: string,
	props: Record<string, unknown>,
): Record<string, string | number | boolean> | null {
	if (!Object.hasOwn(events, event)) return null;
	const fields: Record<string, Field> = events[event as EventName];
	const out: Record<string, string | number | boolean> = {};
	for (const [key, value] of Object.entries(props)) {
		if (value === undefined) continue;
		if (!Object.hasOwn(fields, key)) return null;
		const field = fields[key];
		const ok =
			field === "boolean"
				? typeof value === "boolean"
				: field === "number"
					? typeof value === "number" && Number.isFinite(value)
					: typeof value === "string" && field.includes(value);
		if (!ok) return null;
		out[key] = value as string | number | boolean;
	}
	return out;
}

/** Sends one named event; one that doesn't match its declaration is dropped, never sent. */
export function track<E extends EventName>(
	event: E,
	props: EventProps<E> = {},
) {
	const clean = cleanProps(event, props);
	if (!clean) return;
	// After the first page view: child effects run before <Analytics />'s, and sending earlier
	// would start PostHog anonymous and before route patterns are set.
	void viewed
		.then(() => start(identity))
		.then((ph) => ph?.capture(event, clean));
}

let firstView: () => void = () => {};
const viewed = new Promise<void>((resolve) => {
	firstView = resolve;
});

// ── Scrubbing every outgoing event ───────────────────────────────────────────────────────────────

/** Maps a pathname to its route pattern (`/agent/$conversationId`); set from the router. */
export type PatternOf = (pathname: string) => string;
export const notFoundPattern = "/not-found";
let patternOf: PatternOf = () => notFoundPattern;

export function setRoutePatterns(fn: PatternOf) {
	patternOf = fn;
}

// Referrers can carry another site's codes; titles and error messages can carry what people typed.
const dropped =
	/referr|search_engine|^\$?title$|^\$exception_(values|message|steps)$|^attribution$/i;
// Any address PostHog records (the page, the session's first page, the previous page).
const address = /url|pathname|href/i;
const frameFields = [
	"platform",
	"filename",
	"abs_path",
	"function",
	"module",
	"lineno",
	"colno",
	"in_app",
	"chunk_id",
];

/** An address as its route pattern on this origin; anything else (another site, not a URL) is dropped. */
function scrubAddress(
	key: string,
	value: unknown,
	origin: string,
	pattern: PatternOf,
): string | undefined {
	if (typeof value !== "string") return undefined;
	let url: URL;
	try {
		url = new URL(value, origin);
	} catch {
		return undefined;
	}
	if (url.origin !== origin) return undefined;
	const route = pattern(url.pathname);
	return /pathname/i.test(key) ? route : `${origin}${route}`;
}

/** A script's address without its query or hash; an inline script's "file" is the page's address. */
function scrubFile(value: unknown): unknown {
	return typeof value === "string" ? value.replace(/[?#].*$/, "") : value;
}

/** An exception as its type and stack only: the message can hold anything the page was given. */
function scrubException(item: unknown): unknown {
	if (!item || typeof item !== "object") return undefined;
	const { type, mechanism, stacktrace } = item as Record<string, unknown>;
	const frames = (stacktrace as { frames?: unknown } | undefined)?.frames;
	return {
		type: typeof type === "string" ? type : "Error",
		mechanism,
		stacktrace: Array.isArray(frames)
			? {
					type: (stacktrace as { type?: unknown }).type,
					frames: frames.map((f: Record<string, unknown>) =>
						Object.fromEntries(
							frameFields
								.filter((k) => f && typeof f === "object" && k in f)
								.map((k) => [
									k,
									k === "filename" || k === "abs_path" ? scrubFile(f[k]) : f[k],
								]),
						),
					),
				}
			: undefined,
	};
}

function scrubValue(
	key: string,
	value: unknown,
	origin: string,
	pattern: PatternOf,
): unknown {
	if (dropped.test(key)) return undefined;
	if (address.test(key)) return scrubAddress(key, value, origin, pattern);
	if (key === "$exception_list")
		return Array.isArray(value) ? value.map(scrubException) : undefined;
	if (Array.isArray(value))
		return value.map((v) => scrubValue("", v, origin, pattern));
	if (value && typeof value === "object" && !(value instanceof Date))
		return scrubObject(value as Record<string, unknown>, origin, pattern);
	return value;
}

function scrubObject(
	props: Record<string, unknown>,
	origin: string,
	pattern: PatternOf,
): Record<string, unknown> {
	const out: Record<string, unknown> = {};
	for (const [k, v] of Object.entries(props)) {
		const clean = scrubValue(k, v, origin, pattern);
		if (clean !== undefined) out[k] = clean;
	}
	return out;
}

/**
 * Every event, PostHog's own included, on its way out: addresses become route patterns (no query,
 * hash or ids), referrers and page titles go, and exceptions keep their type and stack only.
 */
export function scrubEvent(
	event: CaptureResult,
	origin: string,
	pattern: PatternOf = patternOf,
): CaptureResult {
	const scrubbed = scrubObject(
		event as unknown as Record<string, unknown>,
		origin,
		pattern,
	);
	return {
		...scrubbed,
		properties: scrubbed.properties ?? {},
	} as unknown as CaptureResult;
}

// ── The client ───────────────────────────────────────────────────────────────────────────────────

/** Who is signed in, as the core's ids: `Me.person.id` (a MemberId) and `Me.org.id` (a TenantId). */
export type Identity = { member: string; org: string };

export function config(who: Identity | null): Partial<PostHogConfig> {
	return {
		// Same origin, so the CSP's connect-src 'self' covers it; the Worker forwards it to the EU.
		api_host: "/ingest",
		ui_host: "https://eu.posthog.com",
		// The bundle has what it needs; the CSP refuses scripts from PostHog's CDN anyway.
		disable_external_dependency_loading: true,
		// No /flags and no remote config, so nothing set in PostHog can switch features on here.
		advanced_disable_flags: true,
		// Nothing written to the device (no cookie, no storage), so no identifier outlives the page;
		// a signed-in page starts as the member (bootstrap below) rather than merging a new id each load.
		persistence: "memory",
		bootstrap: who ? { distinctID: who.member, isIdentifiedID: true } : {},
		person_profiles: "identified_only",
		autocapture: false,
		capture_pageview: false,
		capture_pageleave: false,
		capture_dead_clicks: false,
		capture_heatmaps: false,
		enable_heatmaps: false,
		rageclick: false,
		disable_session_recording: true,
		disable_surveys: true,
		disable_product_tours: true,
		disable_conversations: true,
		disable_web_experiments: true,
		save_referrer: false,
		save_campaign_params: false,
		capture_exceptions: {
			capture_unhandled_errors: true,
			capture_unhandled_rejections: true,
			capture_console_errors: false,
		},
		capture_performance: {
			web_vitals: true,
			web_vitals_attribution: false,
			network_timing: false,
		},
		before_send: (event) => scrubEvent(event, window.location.origin),
	};
}

let client: Promise<PostHog | null> | null = null;
/** Who PostHog is sending as now; null while signed out. */
let identity: Identity | null = null;

/** Loads and starts PostHog once, as `who` when known; null when analytics is off. */
function start(who: Identity | null): Promise<PostHog | null> {
	if (!posthogKey || typeof window === "undefined")
		return Promise.resolve(null);
	client ??= Promise.all([
		import("posthog-js/no-external"),
		// The bundled extensions external loading would otherwise fetch from PostHog's CDN.
		import("posthog-js/dist/exception-autocapture"),
		import("posthog-js/dist/web-vitals"),
	])
		.then(([{ default: posthog }]) => {
			posthog.init(posthogKey, config(who));
			identity = who;
			if (who) posthog.group("organization", who.org);
			return posthog;
		})
		.catch(() => null);
	return client;
}

/** Sends as `who` from now on: identify once after sign-in, a new org's group after switching. */
function becomes(ph: PostHog, who: Identity | null) {
	if (!who) return;
	if (identity?.member !== who.member) {
		if (identity) ph.reset();
		ph.identify(who.member);
	}
	if (identity?.org !== who.org || identity?.member !== who.member)
		ph.group("organization", who.org);
	identity = who;
}

// The last page counted, so a change of search (Settings opening) isn't another page view.
let lastPath: string | null = null;

/** A page was reached: keeps the identity current and counts the view once per path. */
export async function pageView(pathname: string, who: Identity | null) {
	const ph = await start(who);
	if (!ph) return;
	becomes(ph, who);
	firstView();
	if (pathname === lastPath) return;
	lastPath = pathname;
	ph.capture("$pageview");
}

/** Signed out: PostHog forgets the member, and what follows is anonymous again. */
export function signedOut() {
	identity = null;
	void client?.then((ph) => ph?.reset());
}
