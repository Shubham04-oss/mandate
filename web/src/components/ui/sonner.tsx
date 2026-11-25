"use client";

import { cn } from "cn";
import {
	CircleCheckIcon,
	InfoIcon,
	Loader2Icon,
	OctagonXIcon,
	TriangleAlertIcon,
} from "lucide-react";
import { Toaster as Sonner, type ToasterProps } from "sonner";

// Colours come from the theme variables, so the toasts follow `.light` without a theme prop.
const Toaster = ({ ...props }: ToasterProps) => {
	return (
		<Sonner
			className="toaster group"
			icons={{
				success: <CircleCheckIcon className="size-4" />,
				info: <InfoIcon className="size-4" />,
				warning: <TriangleAlertIcon className="size-4" />,
				error: <OctagonXIcon className="size-4" />,
				loading: <Loader2Icon className="size-4 animate-spin" />,
			}}
			style={
				{
					"--normal-bg": "var(--raised)",
					"--normal-text": "var(--text)",
					"--normal-border": "var(--border-quiet)",
					"--border-radius": "16px",
					"--width": "380px",
				} as React.CSSProperties
			}
			toastOptions={{
				classNames: {
					toast:
						"cn-toast !min-h-16 !rounded-section !border-0 !shadow-float !font-sans",
					title: "!text-[12.5px] !font-bold",
					description: "!text-[12px] !text-muted-foreground",
				},
			}}
			{...props}
		/>
	);
};

/**
 * A toast with a strip of painted scene on its leading edge, an avatar over it, and one action.
 * Render it with `toast.custom(() => <SceneToast … />)`.
 */
function SceneToast({
	scene,
	avatar,
	title,
	description,
	action,
	className,
}: {
	scene?: string;
	avatar?: string;
	title: string;
	description?: string;
	action?: React.ReactNode;
	className?: string;
}) {
	return (
		<div
			className={cn(
				"relative flex min-h-16 w-[380px] items-center gap-3 overflow-hidden rounded-section bg-raised py-2.5 pr-3 font-sans text-foreground shadow-float",
				scene ? "pl-[74px]" : "pl-3.5",
				className,
			)}
		>
			{scene && (
				<i
					aria-hidden="true"
					className="scene scene-fade-r scene-dim absolute inset-y-0 left-0 w-24"
					style={{ backgroundImage: `url(${scene})` }}
				/>
			)}
			{avatar && (
				<img
					src={avatar}
					alt=""
					className="absolute top-1/2 left-4 z-10 size-[34px] -translate-y-1/2 rounded-full shadow-[0_0_0_2px_var(--raised)]"
				/>
			)}
			<div className="flex min-w-0 flex-1 flex-col gap-0.5">
				<strong className="text-[12.5px]">{title}</strong>
				{description && (
					<span className="text-[12px] text-muted-foreground">
						{description}
					</span>
				)}
			</div>
			{action}
		</div>
	);
}

export { Toaster, SceneToast };
