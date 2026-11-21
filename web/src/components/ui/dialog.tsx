"use client";

import { Dialog as DialogPrimitive } from "@base-ui/react/dialog";
import { cn } from "cn";
import { XIcon } from "lucide-react";
import type * as React from "react";
import { Button } from "#/components/ui/button.tsx";

function Dialog({ ...props }: DialogPrimitive.Root.Props) {
	return <DialogPrimitive.Root data-slot="dialog" {...props} />;
}

function DialogTrigger({ ...props }: DialogPrimitive.Trigger.Props) {
	return <DialogPrimitive.Trigger data-slot="dialog-trigger" {...props} />;
}

function DialogPortal({ ...props }: DialogPrimitive.Portal.Props) {
	return <DialogPrimitive.Portal data-slot="dialog-portal" {...props} />;
}

function DialogClose({ ...props }: DialogPrimitive.Close.Props) {
	return <DialogPrimitive.Close data-slot="dialog-close" {...props} />;
}

function DialogOverlay({
	className,
	...props
}: DialogPrimitive.Backdrop.Props) {
	return (
		<DialogPrimitive.Backdrop
			data-slot="dialog-overlay"
			className={cn(
				"fixed inset-0 isolate z-50 bg-(--shade-99) duration-150 data-open:animate-in data-open:fade-in-0 data-closed:animate-out data-closed:fade-out-0",
				className,
			)}
			{...props}
		/>
	);
}

function DialogContent({
	className,
	children,
	showCloseButton = true,
	scene,
	...props
}: DialogPrimitive.Popup.Props & {
	showCloseButton?: boolean;
	/** Set when the panel opens with a DialogScene, so the close button reads on the painting. */
	scene?: boolean;
}) {
	return (
		<DialogPortal>
			<DialogOverlay />
			<DialogPrimitive.Popup
				data-slot="dialog-content"
				data-scene={scene || undefined}
				className={cn(
					"fixed top-1/2 left-1/2 z-50 flex w-[420px] max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-2xl bg-popover text-[13px] text-popover-foreground shadow-dialog duration-150 outline-none data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95",
					className,
				)}
				{...props}
			>
				{children}
				{showCloseButton && (
					<DialogPrimitive.Close
						data-slot="dialog-close"
						render={
							<Button
								variant="ghost"
								className="absolute top-3 right-3 z-10 in-data-[scene=true]:bg-black/35 in-data-[scene=true]:text-white in-data-[scene=true]:backdrop-blur-[6px] in-data-[scene=true]:hover:bg-black/50"
								size="icon-sm"
							/>
						}
					>
						<XIcon />
						<span className="sr-only">Close</span>
					</DialogPrimitive.Close>
				)}
			</DialogPrimitive.Popup>
		</DialogPortal>
	);
}

/**
 * Painted scene across the top of the panel. A `band` fades into the panel; a `hero` is taller,
 * keeps its top edge crisp and carries the wordmark. Moments get one (welcome, invite); warnings,
 * errors and destructive confirms never do.
 */
function DialogScene({
	src,
	variant = "band",
	tall,
	position = "center 60%",
	className,
	children,
}: {
	src: string;
	variant?: "band" | "hero";
	tall?: boolean;
	position?: string;
	className?: string;
	children?: React.ReactNode;
}) {
	return (
		<div
			aria-hidden="true"
			data-slot="dialog-scene"
			className={cn(
				"relative shrink-0",
				variant === "hero" ? "h-[200px]" : tall ? "h-[168px]" : "h-28",
				className,
			)}
		>
			<div
				className={cn(
					"scene scene-dim absolute inset-0",
					variant === "band" && "scene-fade-b",
				)}
				style={{ backgroundImage: `url(${src})`, backgroundPosition: position }}
			/>
			{variant === "hero" && (
				// outside the dimmed layer, so it lands exactly on the panel colour
				<div className="absolute inset-0 bg-linear-to-b from-black/5 via-transparent via-40% to-popover to-90%" />
			)}
			{children}
		</div>
	);
}

/** The panel's content; it rises over a scene so the scene fades in behind it. */
function DialogBody({ className, ...props }: React.ComponentProps<"div">) {
	return (
		<div
			data-slot="dialog-body"
			className={cn(
				"relative flex flex-col gap-3.5 px-[22px] pt-5 pb-5 [[data-slot=dialog-scene]+&]:-mt-[34px] [[data-slot=dialog-scene]+&]:pt-0",
				className,
			)}
			{...props}
		/>
	);
}

/** A small raised tile for an icon or an avatar, at the top of the body. */
function DialogIcon({ className, ...props }: React.ComponentProps<"span">) {
	return (
		<span
			data-slot="dialog-icon"
			className={cn(
				"grid size-10 shrink-0 place-items-center rounded-lg bg-raised shadow-icon [&_img]:size-6 [&_img]:rounded-full [&_svg:not([class*='size-'])]:size-[18px]",
				className,
			)}
			{...props}
		/>
	);
}

function DialogHeader({ className, ...props }: React.ComponentProps<"div">) {
	return (
		<div
			data-slot="dialog-header"
			className={cn("flex flex-col gap-1.5", className)}
			{...props}
		/>
	);
}

function DialogFooter({
	className,
	showCloseButton = false,
	children,
	...props
}: React.ComponentProps<"div"> & {
	showCloseButton?: boolean;
}) {
	return (
		<div
			data-slot="dialog-footer"
			className={cn("flex items-center gap-2 pt-1", className)}
			{...props}
		>
			{children}
			{showCloseButton && (
				<DialogPrimitive.Close render={<Button variant="outline" />}>
					Close
				</DialogPrimitive.Close>
			)}
		</div>
	);
}

function DialogTitle({ className, ...props }: DialogPrimitive.Title.Props) {
	return (
		<DialogPrimitive.Title
			data-slot="dialog-title"
			className={cn(
				"m-0 text-[17px] leading-snug font-bold tracking-[-0.01em]",
				className,
			)}
			{...props}
		/>
	);
}

function DialogDescription({
	className,
	...props
}: DialogPrimitive.Description.Props) {
	return (
		<DialogPrimitive.Description
			data-slot="dialog-description"
			className={cn(
				"m-0 text-[13px] leading-[1.55] text-soft-foreground *:[a]:underline *:[a]:underline-offset-3 *:[a]:hover:text-foreground",
				className,
			)}
			{...props}
		/>
	);
}

export {
	Dialog,
	DialogBody,
	DialogClose,
	DialogIcon,
	DialogScene,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogOverlay,
	DialogPortal,
	DialogTitle,
	DialogTrigger,
};
