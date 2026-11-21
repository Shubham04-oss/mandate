// Each connected app's own mark, shown on a white tile. Colours are the brands', not ours.

export function SlackMark({ size = 15 }: { size?: number }) {
	return (
		<svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true">
			<rect x="10" y="2" width="4" height="9" rx="2" fill="#36C5F0" />
			<rect x="13" y="10" width="9" height="4" rx="2" fill="#2EB67D" />
			<rect x="10" y="13" width="4" height="9" rx="2" fill="#ECB22E" />
			<rect x="2" y="10" width="9" height="4" rx="2" fill="#E01E5A" />
		</svg>
	);
}

export function GmailMark({ size = 16 }: { size?: number }) {
	return (
		<svg
			width={size}
			height={(size * 66) / 88}
			viewBox="52 42 88 66"
			aria-hidden="true"
		>
			<path fill="#4285f4" d="M58 108h14V74L52 59v43c0 3.32 2.69 6 6 6" />
			<path fill="#34a853" d="M120 108h14c3.32 0 6-2.69 6-6V59l-20 15" />
			<path fill="#fbbc04" d="M120 48v26l20-15v-8c0-7.42-8.47-11.65-14.4-7.2" />
			<path fill="#ea4335" d="M72 74V48l24 18 24-18v26L96 92" />
			<path
				fill="#c5221f"
				d="M52 51v8l20 15V48l-5.6-4.2c-5.94-4.45-14.4-.22-14.4 7.2"
			/>
		</svg>
	);
}

export function DriveMark({ size = 16 }: { size?: number }) {
	return (
		<svg
			width={size}
			height={(size * 78) / 87.3}
			viewBox="0 0 87.3 78"
			aria-hidden="true"
		>
			<path
				d="m6.6 66.85 3.85 6.65c.8 1.4 1.95 2.5 3.3 3.3l13.75-23.8h-27.5c0 1.55.4 3.1 1.2 4.5z"
				fill="#0066da"
			/>
			<path
				d="m43.65 25-13.75-23.8c-1.35.8-2.5 1.9-3.3 3.3l-25.4 44a9.06 9.06 0 0 0 -1.2 4.5h27.5z"
				fill="#00ac47"
			/>
			<path
				d="m73.55 76.8c1.35-.8 2.5-1.9 3.3-3.3l1.6-2.75 7.65-13.25c.8-1.4 1.2-2.95 1.2-4.5h-27.502l5.852 11.5z"
				fill="#ea4335"
			/>
			<path
				d="m43.65 25 13.75-23.8c-1.35-.8-2.9-1.2-4.5-1.2h-18.5c-1.6 0-3.15.45-4.5 1.2z"
				fill="#00832d"
			/>
			<path
				d="m59.8 53h-32.3l-13.75 23.8c1.35.8 2.9 1.2 4.5 1.2h50.8c1.6 0 3.15-.45 4.5-1.2z"
				fill="#2684fc"
			/>
			<path
				d="m73.4 26.5-12.7-22c-.8-1.4-1.95-2.5-3.3-3.3l-13.75 23.8 16.15 28h27.45c0-1.55-.4-3.1-1.2-4.5z"
				fill="#ffba00"
			/>
		</svg>
	);
}
