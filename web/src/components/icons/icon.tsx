import type { SVGProps } from "react";
import { pika } from "#/components/icons/pika.tsx";

// Glyphs the Pika free set lacks, drawn as in the canvas boards.
// ponytail: replace with Pika equivalents once the full set is in.
const extra = {
	eye: (
		<>
			<path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0" />
			<circle cx="12" cy="12" r="3" />
		</>
	),
	"eye-off": (
		<>
			<path d="M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49" />
			<path d="M14.084 14.158a3 3 0 0 1-4.242-4.242" />
			<path d="M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143" />
			<path d="m2 2 20 20" />
		</>
	),
	check: <path d="M20 6 9 17l-5-5" />,
	// from the Oryxa Website Language icons, drawn to the Pika rules
	moon: (
		<path d="M20.6 14.6C19.5 15.06 18.29 15.32 17.02 15.32C11.9 15.32 7.75 11.17 7.75 6.05C7.75 4.92 7.95 3.84 8.32 2.84C4.66 4.11 2 7.6 2 11.7C2 16.94 6.25 21.18 11.48 21.18C15.79 21.18 19.43 18.31 20.6 14.6Z" />
	),
	dot: <circle cx="12" cy="12" r="4" />,
};

const glyphs = { ...pika, ...extra };

export type IconName = keyof typeof glyphs;

type IconProps = Omit<SVGProps<SVGSVGElement>, "name"> & {
	name: IconName;
	size?: number;
	/** Set when the icon carries meaning on its own; otherwise it is hidden from screen readers. */
	label?: string;
};

// The one place that knows the icon set, so swapping sets is a change here and in pika.tsx.
export function Icon({
	name,
	size = 16,
	strokeWidth = 2,
	label,
	...props
}: IconProps) {
	return (
		<svg
			width={size}
			height={size}
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			strokeWidth={strokeWidth}
			strokeLinecap="round"
			strokeLinejoin="round"
			role={label ? "img" : undefined}
			aria-label={label}
			aria-hidden={label ? undefined : true}
			{...props}
		>
			{glyphs[name]}
		</svg>
	);
}

export function GoogleMark({ size = 18 }: { size?: number }) {
	return (
		<svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
			<path
				fill="#4285F4"
				d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"
			/>
			<path
				fill="#34A853"
				d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
			/>
			<path
				fill="#FBBC05"
				d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"
			/>
			<path
				fill="#EA4335"
				d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"
			/>
		</svg>
	);
}
