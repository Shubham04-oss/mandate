import { Link } from "@tanstack/react-router";
import { Icon } from "#/components/icon.tsx";
import { footerNav, mainNav, type NavItem } from "#/components/nav.ts";
import {
	Sidebar,
	SidebarContent,
	SidebarFooter,
	SidebarGroup,
	SidebarGroupContent,
	SidebarHeader,
	SidebarMenu,
	SidebarMenuButton,
	SidebarMenuItem,
	SidebarRail,
	useSidebar,
} from "#/components/ui/sidebar.tsx";

function NavList({ items }: { items: NavItem[] }) {
	const { setOpenMobile } = useSidebar();
	return (
		<SidebarMenu>
			{items.map((item) => (
				<SidebarMenuItem key={item.to}>
					<SidebarMenuButton
						tooltip={item.title}
						render={
							<Link
								to={item.to}
								activeOptions={{ exact: item.to === "/" }}
								activeProps={{ "data-active": true }}
								onClick={() => setOpenMobile(false)}
							/>
						}
					>
						<Icon name={item.icon} />
						<span>{item.title}</span>
					</SidebarMenuButton>
				</SidebarMenuItem>
			))}
		</SidebarMenu>
	);
}

export function AppSidebar() {
	return (
		<Sidebar collapsible="icon">
			<SidebarHeader>
				<div className="flex h-8 items-center gap-2 px-2 group-data-[collapsible=icon]:px-1.5">
					<span className="size-5 shrink-0 rounded-sm bg-primary" />
					<span className="font-medium text-sm tracking-tight group-data-[collapsible=icon]:hidden">
						Mandate
					</span>
				</div>
			</SidebarHeader>
			<SidebarContent>
				<SidebarGroup>
					<SidebarGroupContent>
						<NavList items={mainNav} />
					</SidebarGroupContent>
				</SidebarGroup>
			</SidebarContent>
			<SidebarFooter>
				<NavList items={footerNav} />
			</SidebarFooter>
			<SidebarRail />
		</Sidebar>
	);
}
