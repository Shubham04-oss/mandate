import { useState } from "react";
import {
	SettingRow,
	SettingsCard,
	SettingsGroup,
	SettingsHeader,
} from "#/components/settings/parts.tsx";
import { Button } from "#/components/ui/button.tsx";
import { Switch } from "#/components/ui/switch.tsx";
import { brand } from "#/lib/brand.ts";
import { desktopNotifications, usePref, writePref } from "#/lib/preferences.ts";

type Permission = NotificationPermission | "unsupported";

const currentPermission = (): Permission =>
	typeof Notification === "undefined" ? "unsupported" : Notification.permission;

// ponytail: only the test sends a notification for now. The contract has no event stream, so the
// web app can't hear when something starts needing the person; noted for the lead.
export function NotificationsSection() {
	return (
		<div className="flex flex-col gap-8">
			<SettingsHeader eyebrow="On this device" title="Notifications">
				Notifications this browser shows. Your other devices keep their own.
			</SettingsHeader>
			<SettingsGroup id="desktop-h" title="Desktop">
				<SettingsCard>
					<DesktopRows />
				</SettingsCard>
			</SettingsGroup>
		</div>
	);
}

function DesktopRows() {
	const wanted = usePref(desktopNotifications);
	const [permission, setPermission] = useState(currentPermission);
	const on = wanted && permission === "granted";

	const toggle = async (next: boolean) => {
		if (!next || permission === "unsupported") {
			writePref(desktopNotifications, false);
			return;
		}
		// the browser asks the person once; after that it remembers their answer
		const answer =
			permission === "granted"
				? permission
				: await Notification.requestPermission();
		setPermission(answer);
		writePref(desktopNotifications, answer === "granted");
	};

	const sendTest = () => {
		new Notification(`${brand.name}: something needs you`, {
			body: "This is how a request that needs you will appear on this device.",
		});
	};

	return (
		<>
			<SettingRow
				title="Desktop notifications"
				description="When something needs you, this browser shows a notification, even with oryxa in another tab."
				control={
					<Switch
						aria-label="Desktop notifications"
						checked={on}
						disabled={permission === "denied" || permission === "unsupported"}
						onCheckedChange={toggle}
					/>
				}
				note={notes[permission]}
			/>
			<SettingRow
				title="Send a test"
				description="See how a notification looks here."
				control={
					<Button variant="outline" size="sm" disabled={!on} onClick={sendTest}>
						Send a test
					</Button>
				}
			/>
		</>
	);
}

const notes: Partial<Record<Permission, string>> = {
	denied:
		"This browser blocks notifications from oryxa. Allow them in the browser's settings for this site, then turn this on.",
	unsupported: "This browser can't show desktop notifications.",
};
