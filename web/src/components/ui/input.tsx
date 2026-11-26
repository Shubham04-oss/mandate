import { Input as InputPrimitive } from "@base-ui/react/input";
import { cn } from "cn";
import type * as React from "react";

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
	return (
		<InputPrimitive
			type={type}
			data-slot="input"
			className={cn(
				"h-11 w-full min-w-0 rounded-lg border-0 bg-canvas px-3.5 text-[13.5px] text-foreground shadow-field transition-shadow duration-150 outline-none placeholder:text-muted-foreground focus-visible:shadow-field-focus disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:shadow-field-invalid",
				className,
			)}
			{...props}
		/>
	);
}

export { Input };
