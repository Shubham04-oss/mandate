import type { FormEvent } from "react";
import { AppLogo } from "#/components/settings/app-logo.tsx";
import { Button } from "#/components/ui/button.tsx";
import {
	Dialog,
	DialogBody,
	DialogClose,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogIcon,
	DialogTitle,
} from "#/components/ui/dialog.tsx";
import { Field, FieldDescription, FieldLabel } from "#/components/ui/field.tsx";
import { Input } from "#/components/ui/input.tsx";
import type { CatalogTool } from "#/lib/tools.ts";

// ponytail: the key goes to the core, which checks it with the app and stores it in the token vault;
// until that exists the form only closes.
const notWired = (e: FormEvent) => e.preventDefault();

/** For apps that connect with a key the person pastes, rather than by signing in. */
export function ConnectKeyDialog({
	tool,
	open,
	onOpenChange,
}: {
	tool: CatalogTool;
	open: boolean;
	onOpenChange: (open: boolean) => void;
}) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<DialogBody>
					<DialogIcon className="overflow-hidden p-0">
						<AppLogo logo={tool.logo} name={tool.name} size={40} />
					</DialogIcon>
					<DialogHeader>
						<DialogTitle>Connect {tool.name}</DialogTitle>
						<DialogDescription>
							The key goes straight to the token vault. Agents use it on your
							behalf and never see it.
						</DialogDescription>
					</DialogHeader>
					<form
						id={`connect-${tool.id}`}
						onSubmit={notWired}
						className="flex flex-col gap-3"
					>
						{tool.keyFields?.map((field) => {
							const id = `${tool.id}-${field.id}`;
							return (
								<Field key={field.id} className="gap-1.5">
									<FieldLabel htmlFor={id}>{field.label}</FieldLabel>
									<Input
										id={id}
										name={field.id}
										type={field.secret ? "password" : "text"}
										autoComplete="off"
										className="h-10"
									/>
									{field.help ? (
										<FieldDescription className="text-[11.5px]">
											{field.help}
										</FieldDescription>
									) : null}
								</Field>
							);
						})}
					</form>
					<DialogFooter className="justify-end">
						<DialogClose render={<Button variant="outline" />}>
							Cancel
						</DialogClose>
						<Button type="submit" form={`connect-${tool.id}`}>
							Connect
						</Button>
					</DialogFooter>
				</DialogBody>
			</DialogContent>
		</Dialog>
	);
}
