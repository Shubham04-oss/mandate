import { mergeProps } from "@base-ui/react/merge-props";
import { useRender } from "@base-ui/react/use-render";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "cn";

const badgeVariants = cva(
	// bare: a word (and its mark or icon) in its colour, with no pill or outline behind it
	"group/badge inline-flex w-fit shrink-0 items-center gap-1.5 rounded-sm text-[12px] leading-tight font-semibold whitespace-nowrap focus-visible:ring-[3px] focus-visible:ring-ring/50 [&>svg]:pointer-events-none [&>svg]:size-3.5!",
	{
		variants: {
			variant: {
				default: "text-foreground",
				secondary: "text-[12.5px] text-foreground",
				success: "text-ok-foreground",
				destructive: "text-danger-foreground",
				outline: "font-medium text-soft-foreground",
				ghost: "font-medium text-muted-foreground",
				link: "text-primary underline-offset-4 hover:underline",
			},
		},
		defaultVariants: {
			variant: "default",
		},
	},
);

function Badge({
	className,
	variant = "default",
	render,
	...props
}: useRender.ComponentProps<"span"> & VariantProps<typeof badgeVariants>) {
	return useRender({
		defaultTagName: "span",
		props: mergeProps<"span">(
			{
				className: cn(badgeVariants({ variant }), className),
			},
			props,
		),
		render,
		state: {
			slot: "badge",
			variant,
		},
	});
}

export { Badge, badgeVariants };
