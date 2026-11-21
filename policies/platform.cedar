// An agent may attempt a tool only under a live grant.
permit (principal is Agent, action == Action::"invoke", resource is Tool)
when { context.granted };

// Reads under a grant run without asking; other effects need a standing grant.
permit (principal is Agent, action == Action::"invoke_unattended", resource is Tool)
when { context.granted && (resource.effect == "read" || context.standing) };

// Anything leaving the company or deleting is never unattended, whatever the grant.
forbid (principal, action == Action::"invoke_unattended", resource is Tool)
when { resource.effect == "external" || resource.effect == "delete" };
