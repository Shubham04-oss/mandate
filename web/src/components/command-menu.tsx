import { useNavigate } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { Icon } from "#/components/icon.tsx";
import { footerNav, mainNav } from "#/components/nav.ts";
import { Button } from "#/components/ui/button.tsx";
import {
	Command,
	CommandDialog,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
} from "#/components/ui/command.tsx";

export function CommandMenu() {
	const [open, setOpen] = useState(false);
	const navigate = useNavigate();

	useEffect(() => {
		const onKeyDown = (event: KeyboardEvent) => {
			if (event.key.toLowerCase() === "k" && (event.metaKey || event.ctrlKey)) {
				event.preventDefault();
				setOpen((current) => !current);
			}
		};
		document.addEventListener("keydown", onKeyDown);
		return () => document.removeEventListener("keydown", onKeyDown);
	}, []);

	return (
		<>
			<Button
				variant="outline"
				size="sm"
				className="w-56 justify-start text-muted-foreground max-sm:w-auto"
				onClick={() => setOpen(true)}
			>
				<Icon name="search" />
				<span className="max-sm:hidden">Search</span>
				<kbd className="ml-auto font-sans text-xs max-sm:hidden">Ctrl K</kbd>
			</Button>
			<CommandDialog open={open} onOpenChange={setOpen} title="Search">
				<Command>
					<CommandInput placeholder="Go to..." />
					<CommandList>
						<CommandEmpty>Nothing found.</CommandEmpty>
						<CommandGroup heading="Go to">
							{[...mainNav, ...footerNav].map((item) => (
								<CommandItem
									key={item.to}
									onSelect={() => {
										setOpen(false);
										navigate({ to: item.to });
									}}
								>
									<Icon name={item.icon} />
									{item.title}
								</CommandItem>
							))}
						</CommandGroup>
					</CommandList>
				</Command>
			</CommandDialog>
		</>
	);
}
