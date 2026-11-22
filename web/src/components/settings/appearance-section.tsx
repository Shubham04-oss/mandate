import { useState } from "react";
import {
	SettingRow,
	SettingsCard,
	SettingsGroup,
	SettingsHeader,
} from "#/components/settings/parts.tsx";
import { RadioGroup, RadioGroupItem } from "#/components/ui/radio-group.tsx";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "#/components/ui/select.tsx";
import { Switch } from "#/components/ui/switch.tsx";
import {
	type Clock,
	clock,
	formatTime,
	reduceMotion,
	usePref,
	writePref,
} from "#/lib/preferences.ts";
import {
	chooseTheme,
	parseChoice,
	readChoice,
	type ThemeChoice,
	themeChoices,
} from "#/lib/theme.ts";

export function AppearanceSection() {
	return (
		<div className="flex flex-col gap-8">
			<SettingsHeader eyebrow="On this device" title="Appearance">
				How oryxa looks in this browser. Your other devices keep their own.
			</SettingsHeader>
			<SettingsGroup id="theme-h" title="Theme">
				<ThemeCards />
			</SettingsGroup>
			<SettingsGroup id="display-h" title="Motion and time">
				<SettingsCard>
					<MotionRow />
					<ClockRow />
				</SettingsCard>
			</SettingsGroup>
		</div>
	);
}

function ThemeCards() {
	const [choice, setChoice] = useState(readChoice);
	return (
		<RadioGroup
			aria-labelledby="theme-h"
			value={choice}
			onValueChange={(value) => {
				const next = parseChoice(value);
				setChoice(next);
				chooseTheme(next);
			}}
			className="grid grid-cols-3 gap-3 max-sm:gap-2"
		>
			{themeChoices.map((c) => (
				// biome-ignore lint/a11y/noLabelWithoutControl: the radio inside is the control
				<label
					key={c.value}
					className="flex cursor-pointer flex-col gap-2.5 rounded-card bg-raised p-2 shadow-card transition-shadow duration-150 hover:shadow-card-hover has-data-checked:shadow-[0_0_0_2px_var(--text)]"
				>
					<ThemePreview choice={c.value} />
					<span className="flex items-center gap-2.5 px-1.5 pb-1 font-semibold text-[13px]">
						<RadioGroupItem value={c.value} />
						{c.label}
					</span>
				</label>
			))}
		</RadioGroup>
	);
}

// Fixed colours: each preview shows its own theme whichever one is on.
const palettes = {
	light: {
		frame: "#f5f5f4",
		canvas: "#fafaf9",
		card: "#ffffff",
		line: "#e7e5e4",
		ink: "#111111",
		soft: "#d6d3d1",
	},
	dark: {
		frame: "#111111",
		canvas: "#1e1e1e",
		card: "#242424",
		line: "#353535",
		ink: "#f2f2f2",
		soft: "#4a4a4a",
	},
};

/** A small drawing of the app: rail, canvas, a card with an eyebrow and two lines. */
function Window({ tone }: { tone: "light" | "dark" }) {
	const p = palettes[tone];
	return (
		<div
			className="absolute inset-0 flex gap-1.5 p-1.5"
			style={{ background: p.frame }}
		>
			<div className="flex w-2.5 flex-col gap-1 pt-1">
				{[0, 1, 2].map((i) => (
					<i
						key={i}
						className="block size-2 rounded-[3px]"
						style={{ background: i === 0 ? p.soft : p.line }}
					/>
				))}
			</div>
			<div
				className="flex flex-1 items-center justify-center rounded-t-[8px]"
				style={{ background: p.canvas }}
			>
				<div
					className="flex w-3/4 flex-col gap-1.5 rounded-[7px] p-2"
					style={{ background: p.card, boxShadow: `0 0 0 1px ${p.line}` }}
				>
					<span className="flex items-center gap-1">
						<i className="block h-px w-2 bg-gold-deep" />
						<i
							className="block h-1 w-6 rounded-full"
							style={{ background: p.soft }}
						/>
					</span>
					<i
						className="block h-1.5 w-4/5 rounded-full"
						style={{ background: p.ink }}
					/>
					<i
						className="block h-1 w-3/5 rounded-full"
						style={{ background: p.soft }}
					/>
				</div>
			</div>
		</div>
	);
}

function ThemePreview({ choice }: { choice: ThemeChoice }) {
	return (
		<div
			aria-hidden="true"
			className="relative aspect-[16/10] overflow-hidden rounded-[14px] shadow-[0_0_0_1px_var(--border-quiet)]"
		>
			{choice === "dark" ? <Window tone="dark" /> : <Window tone="light" />}
			{choice === "system" ? (
				// the device decides: light on one side, dark on the other
				<div className="absolute inset-0 [clip-path:polygon(55%_0,100%_0,100%_100%,45%_100%)]">
					<Window tone="dark" />
				</div>
			) : null}
		</div>
	);
}

function MotionRow() {
	const on = usePref(reduceMotion);
	return (
		<SettingRow
			title="Reduce motion"
			description="Calms the app's animations. When your device asks for less motion, they're calm anyway."
			control={
				<Switch
					aria-label="Reduce motion"
					checked={on}
					onCheckedChange={(next) => writePref(reduceMotion, next)}
				/>
			}
		/>
	);
}

// An afternoon time shows the difference between the formats best.
const afternoon = new Date(2026, 0, 1, 14, 30).toISOString();

const clockOptions: { value: Clock; label: string }[] = [
	{ value: "system", label: `System (${formatTime(afternoon, "system")})` },
	{ value: "12", label: `12-hour (${formatTime(afternoon, "12")})` },
	{ value: "24", label: `24-hour (${formatTime(afternoon, "24")})` },
];

function ClockRow() {
	const value = usePref(clock);
	return (
		<SettingRow
			title="Time format"
			description="For times across oryxa, such as in Recent."
			control={
				<Select
					value={value}
					onValueChange={(next) => {
						if (next) writePref(clock, clock.parse(next));
					}}
					items={clockOptions}
				>
					<SelectTrigger aria-label="Time format" className="min-w-40">
						<SelectValue />
					</SelectTrigger>
					<SelectContent>
						{clockOptions.map((o) => (
							<SelectItem key={o.value} value={o.value}>
								{o.label}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			}
		/>
	);
}
