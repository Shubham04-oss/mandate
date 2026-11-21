import { createFileRoute, Outlet } from "@tanstack/react-router";
import { AppSidebar } from "#/components/app-sidebar.tsx";
import { CommandMenu } from "#/components/command-menu.tsx";
import {
	SidebarInset,
	SidebarProvider,
	SidebarTrigger,
} from "#/components/ui/sidebar.tsx";

export const Route = createFileRoute("/_app")({ component: AppShell });

function AppShell() {
	return (
		<SidebarProvider>
			<AppSidebar />
			<SidebarInset>
				<header className="sticky top-0 z-10 flex h-12 items-center gap-2 border-b bg-background px-3">
					<SidebarTrigger />
					<div className="ml-auto">
						<CommandMenu />
					</div>
				</header>
				<Outlet />
			</SidebarInset>
		</SidebarProvider>
	);
}
