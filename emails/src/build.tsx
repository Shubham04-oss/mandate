// Renders each template to dist/<name>.html (dark), <name>.light.html and <name>.txt with
// {{placeholders}} the core fills in, and with --preview, to preview/ filled with sample values.
import { mkdir, writeFile } from "node:fs/promises";
import { render } from "@react-email/components";
import type { ComponentType } from "react";
import { forOutlook } from "./outlook.ts";
import ApprovalRequested, * as approvalRequested from "./templates/approval-requested.tsx";
import Invitation, * as invitation from "./templates/invitation.tsx";
import ResetPassword, * as resetPassword from "./templates/reset-password.tsx";
import SignInCode, * as signInCode from "./templates/sign-in-code.tsx";
import VerifyEmail, * as verifyEmail from "./templates/verify-email.tsx";
import { type Field, FieldContext } from "./theme.ts";

type Props = Record<string, string>;
type Template = { subject: string; component: ComponentType<Props>; sample: Props };

const templates: Record<string, Template> = {
  "sign-in-code": {
    subject: signInCode.subject,
    component: SignInCode as ComponentType<Props>,
    sample: { email: "priya@acme.com", code: "482913", expiresIn: "10 minutes" },
  },
  "verify-email": {
    subject: verifyEmail.subject,
    component: VerifyEmail as ComponentType<Props>,
    sample: { email: "priya@acme.com", code: "730418", expiresIn: "24 hours" },
  },
  "reset-password": {
    subject: resetPassword.subject,
    component: ResetPassword as ComponentType<Props>,
    sample: {
      email: "priya@acme.com",
      url: "https://app.oryxa.in/reset-password?token=sample",
      expiresIn: "15 minutes",
    },
  },
  invitation: {
    subject: invitation.subject,
    component: Invitation as ComponentType<Props>,
    sample: {
      email: "raya@acme.com",
      inviter: "Priya Shah",
      organization: "Acme",
      url: "https://app.oryxa.in/invite?token=sample",
      expiresIn: "7 days",
    },
  },
  "approval-requested": {
    subject: approvalRequested.subject,
    component: ApprovalRequested as ComponentType<Props>,
    sample: {
      email: "raya@acme.com",
      requester: "Priya Shah",
      agent: "Raya's agent",
      tool: "Read a page on acme.atlassian.net",
      organization: "Acme",
      url: "https://app.oryxa.in/approvals",
      expiresIn: "1 hour",
    },
  },
};

const snake = (k: string) => k.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
const fill = (s: string, props: Props) =>
  s.replace(
    /\{\{(\w+)\}\}/g,
    (_, k: string) => props[k.replace(/_(\w)/g, (_m, c) => c.toUpperCase())] ?? "",
  );

// The plain-text part: no picture or decoration, and the code or link on a line of its own.
async function plainText(el: React.ReactElement) {
  const text = await render(el, {
    plainText: true,
    htmlToTextOptions: {
      selectors: [
        { selector: "img", format: "skip" },
        { selector: "a", options: { hideLinkHrefIfSameAsText: true, linkBrackets: [" : ", ""] } },
        // The details keep their label and value side by side.
        { selector: "table.ledger", format: "dataTable" },
      ],
    },
  });
  return `${text
    // "Choose a new password:" then the link alone on the next line, easy to tap or copy.
    .replace(/ : (\S+)$/gm, ":\n$1")
    .replace(/\n{3,}/g, "\n\n")
    .trim()}\n`;
}

const preview = process.argv.includes("--preview");
const assets = preview ? "assets" : (process.env.EMAIL_ASSETS ?? "https://app.oryxa.in/email");
const out = preview ? "preview" : "dist";
await mkdir(out, { recursive: true });

const manifest: Record<string, { subject: string; placeholders: string[] }> = {};
for (const [name, t] of Object.entries(templates)) {
  const keys = Object.keys(t.sample);
  const props: Props = preview
    ? t.sample
    : Object.fromEntries(keys.map((k) => [k, `{{${snake(k)}}}`]));
  const subject = preview ? fill(t.subject, t.sample) : t.subject;
  for (const field of ["dark", "light"] as Field[]) {
    const el = (
      <FieldContext.Provider value={field}>
        <t.component assets={assets} {...props} />
      </FieldContext.Provider>
    );
    const html = field === "dark" ? `${name}.html` : `${name}.light.html`;
    await writeFile(`${out}/${html}`, forOutlook(await render(el)));
    // Both fields carry the same words, so one plain-text part serves either.
    if (field === "dark") await writeFile(`${out}/${name}.txt`, await plainText(el));
  }
  manifest[name] = { subject, placeholders: keys.map(snake) };
}
await writeFile(`${out}/manifest.json`, `${JSON.stringify(manifest, null, 2)}\n`);
