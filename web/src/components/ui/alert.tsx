import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "cn";
import type * as React from "react";

/**
 * A notice: the same rimmed container as a card. A bare icon or status mark carries the colour,
 * then a title, one line, and at most one action. Status marks keep their own colour.
 */
const alertVariants = cva(
	"group/alert rim relative grid w-full items-center gap-0.5 rounded-[22px] bg-raised px-4 py-3 text-left text-[13px] text-soft-foreground leading-normal has-data-[slot=alert-action]:relative has-data-[slot=alert-action]:pr-32 has-[>svg]:grid-cols-[auto_1fr] has-[>svg]:gap-x-3 *:[svg]:row-span-2 *:[svg:not([class*='size-']):not([data-mark])]:size-[18px]",
	{
		variants: {
			variant: {
				default: "*:[svg:not([data-mark])]:text-muted-foreground",
				destructive: "*:[svg:not([data-mark])]:text-danger-foreground",
			},
		},
		defaultVariants: {
			variant: "default",
		},
	},
);

function Alert({
	className,
	variant,
	...props
}: React.ComponentProps<"div"> & VariantProps<typeof alertVariants>) {
	return (
		<div
			data-slot="alert"
			role="alert"
			className={cn(alertVariants({ variant }), className)}
			{...props}
		/>
	);
}

function AlertTitle({ className, ...props }: React.ComponentProps<"div">) {
	return (
		<div
			data-slot="alert-title"
			className={cn(
				"font-semibold text-foreground group-has-[>svg]/alert:col-start-2 [&_a]:underline [&_a]:underline-offset-3 [&_a]:hover:text-foreground",
				className,
			)}
			{...props}
		/>
	);
}

function AlertDescription({
	className,
	...props
}: React.ComponentProps<"div">) {
	return (
		<div
			data-slot="alert-description"
			className={cn(
				"text-pretty text-soft-foreground [&_a]:underline [&_a]:underline-offset-3 [&_a]:hover:text-foreground [&_p:not(:last-child)]:mb-4",
				className,
			)}
			{...props}
		/>
	);
}

function AlertAction({ className, ...props }: React.ComponentProps<"div">) {
	return (
		<div
			data-slot="alert-action"
			className={cn("-translate-y-1/2 absolute top-1/2 right-3", className)}
			{...props}
		/>
	);
}

export { Alert, AlertTitle, AlertDescription, AlertAction, alertVariants };
