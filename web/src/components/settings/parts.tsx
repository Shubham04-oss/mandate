import type { ReactNode } from "react";

/** A settings page opens the way a website section does: eyebrow, heading, one line of lead. */
export function SettingsHeader({
	eyebrow,
	title,
	children,
}: {
	eyebrow: string;
	title: string;
	children: ReactNode;
}) {
	return (
		<header className="flex flex-col gap-3">
			<span className="eyebrow">{eyebrow}</span>
			<h2 className="m-0 font-semibold text-[22px] tracking-[-0.016em]">
				{title}
			</h2>
			<p className="m-0 max-w-[560px] text-[13.5px] text-soft-foreground leading-normal">
				{children}
			</p>
		</header>
	);
}

/** A titled group of settings in one card, rows divided by a line. */
export function SettingsGroup({
	id,
	title,
	children,
}: {
	id: string;
	title: string;
	children: ReactNode;
}) {
	return (
		<section aria-labelledby={id} className="flex flex-col gap-2.5">
			<h3 id={id} className="m-0 font-semibold text-[16px] tracking-[-0.01em]">
				{title}
			</h3>
			{children}
		</section>
	);
}

export function SettingsCard({ children }: { children: ReactNode }) {
	return (
		<ul className="m-0 flex list-none flex-col rounded-card bg-raised p-0 shadow-card">
			{children}
		</ul>
	);
}

/** One setting: what it does on the left, its control on the right, a note under both if needed. */
export function SettingRow({
	title,
	description,
	control,
	note,
}: {
	title: ReactNode;
	description: ReactNode;
	control: ReactNode;
	note?: ReactNode;
}) {
	return (
		<li className="flex flex-col gap-2.5 border-border border-t px-4 py-3.5 first:border-t-0">
			<div className="flex items-center gap-4">
				<div className="flex min-w-0 flex-1 flex-col gap-0.5">
					<span className="font-semibold text-[13px]">{title}</span>
					<span className="text-[12px] text-muted-foreground leading-normal">
						{description}
					</span>
				</div>
				{control}
			</div>
			{note ? (
				<p className="m-0 rounded-lg bg-hover px-3 py-2.5 text-[12px] text-soft-foreground leading-normal">
					{note}
				</p>
			) : null}
		</li>
	);
}
