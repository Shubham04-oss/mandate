// ponytail: placeholder page body until each section has real content.
export function Page({ title, hint }: { title: string; hint: string }) {
	return (
		<div className="mx-auto w-full max-w-5xl p-6">
			<h1 className="font-medium text-xl tracking-tight">{title}</h1>
			<div className="mt-6 rounded-xl border border-dashed p-10 text-center text-muted-foreground text-sm">
				{hint}
			</div>
		</div>
	);
}
