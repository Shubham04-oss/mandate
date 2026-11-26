import {
	SettingRow,
	SettingsCard,
	SettingsGroup,
	SettingsHeader,
} from "#/components/settings/parts.tsx";
import { Switch } from "#/components/ui/switch.tsx";

// ponytail: off until real alerts exist. Events stream per conversation only, so the web app can't
// hear when something new needs the person; turning this on would promise what it can't do.
export function NotificationsSection() {
	return (
		<div className="flex flex-col gap-8">
			<SettingsHeader eyebrow="On this device" title="Notifications">
				Notifications this browser shows. Your other devices keep their own.
			</SettingsHeader>
			<SettingsGroup id="desktop-h" title="Desktop">
				<SettingsCard>
					<SettingRow
						title="Desktop notifications"
						description="When something needs you, this browser shows a notification, even with oryxa in another tab."
						control={
							<Switch
								aria-label="Desktop notifications"
								checked={false}
								disabled
							/>
						}
						note="Coming soon. oryxa doesn't send desktop notifications yet."
					/>
				</SettingsCard>
			</SettingsGroup>
		</div>
	);
}
