/** The person's theme. "system" follows the device's light or dark setting. */
export type ThemeChoice = "system" | "light" | "dark";

export const themeChoices: { value: ThemeChoice; label: string }[] = [
	{ value: "system", label: "System" },
	{ value: "light", label: "Light" },
	{ value: "dark", label: "Dark" },
];

const storageKey = "mandate.theme";

/** A stored choice; anything unknown means "system". */
export function parseChoice(stored: unknown): ThemeChoice {
	return stored === "light" || stored === "dark" ? stored : "system";
}

export function wantsLight(choice: ThemeChoice, deviceLight: boolean): boolean {
	return choice === "light" || (choice === "system" && deviceLight);
}

export function readChoice(): ThemeChoice {
	try {
		return parseChoice(window.localStorage.getItem(storageKey));
	} catch {
		return "system";
	}
}

/** Keeps the choice on this device and repaints now. */
export function chooseTheme(choice: ThemeChoice) {
	try {
		window.localStorage.setItem(storageKey, choice);
	} catch {
		// storage off: the choice holds until the page reloads
	}
	const deviceLight = window.matchMedia(
		"(prefers-color-scheme: light)",
	).matches;
	document.documentElement.classList.toggle(
		"light",
		wantsLight(choice, deviceLight),
	);
}

/**
 * Runs in the page head before the first paint, so the page never flashes the other theme, and
 * follows the device when it changes. It can't import this module, so it repeats parseChoice and
 * wantsLight; the tests run it against both.
 */
export const themeScript = `(() => {
	const device = window.matchMedia("(prefers-color-scheme: light)");
	const apply = () => {
		let stored = null;
		try { stored = window.localStorage.getItem("${storageKey}"); } catch {}
		const choice = stored === "light" || stored === "dark" ? stored : "system";
		document.documentElement.classList.toggle("light", choice === "light" || (choice === "system" && device.matches));
	};
	apply();
	device.addEventListener("change", apply);
})();`;
