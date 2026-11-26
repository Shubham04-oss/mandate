import { Button as ButtonPrimitive } from "@base-ui/react/button";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "cn";

const buttonVariants = cva(
	"group/button relative inline-flex shrink-0 cursor-pointer items-center justify-center rounded-full border-0 bg-clip-padding font-semibold whitespace-nowrap no-underline transition-[background-color,box-shadow,color] duration-150 outline-none select-none focus-visible:ring-3 focus-visible:ring-ring/50 active:not-aria-[haspopup]:translate-y-px disabled:pointer-events-none disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
	{
		variants: {
			variant: {
				// near-white on dark: the one strong action in a view
				default: "bg-primary text-primary-foreground hover:bg-primary/88",
				// a quiet pill on a raised surface
				outline:
					"bg-canvas text-foreground shadow-field hover:bg-hover aria-expanded:bg-hover",
				// a tile one step above the page
				secondary:
					"bg-raised text-foreground shadow-raised hover:shadow-[0_0_0_1px_var(--border-strong)] aria-expanded:shadow-[0_0_0_1px_var(--border-strong)]",
				ghost:
					"bg-transparent text-muted-foreground hover:bg-hover hover:text-foreground aria-expanded:bg-hover",
				destructive:
					"bg-destructive text-white hover:bg-destructive/88 focus-visible:ring-destructive/40",
				success: "bg-ok-solid text-white hover:bg-ok-solid/88",
				// the round close button that sits on a painted scene
				scene: "bg-black/35 text-white backdrop-blur-[6px] hover:bg-black/50",
				link: "rounded-none text-foreground underline-offset-4 hover:underline",
			},
			size: {
				default: "h-[34px] gap-1.5 px-3.5 text-[12.5px]",
				xs: "h-[26px] gap-1 px-2.5 text-[11.5px] [&_svg:not([class*='size-'])]:size-3",
				sm: "h-[30px] gap-1.5 px-3 text-[12px] [&_svg:not([class*='size-'])]:size-3.5",
				// full-width actions on the sign-in pages
				md: "h-11 gap-2 px-5 text-[13px]",
				lg: "h-[46px] gap-2.5 px-5 text-[13.5px]",
				icon: "size-8",
				"icon-xs": "size-6 [&_svg:not([class*='size-'])]:size-3",
				"icon-sm": "size-7 [&_svg:not([class*='size-'])]:size-3.5",
				"icon-lg": "size-10",
			},
		},
		defaultVariants: {
			variant: "default",
			size: "default",
		},
	},
);

function Button({
	className,
	variant = "default",
	size = "default",
	...props
}: ButtonPrimitive.Props & VariantProps<typeof buttonVariants>) {
	return (
		<ButtonPrimitive
			data-slot="button"
			className={cn(buttonVariants({ variant, size, className }))}
			{...props}
		/>
	);
}

export { Button, buttonVariants };
