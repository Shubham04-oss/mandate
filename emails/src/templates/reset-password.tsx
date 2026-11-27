import { Action, GlassNote, Layout } from "../layout.tsx";

export const subject = "Reset your oryxa password";

export default function ResetPassword(p: {
  assets: string;
  email: string;
  url: string;
  expiresIn: string;
}) {
  return (
    <Layout
      assets={p.assets}
      title="Reset your password"
      preview="Choose a new password for your account."
      scene="sky-veil-v2"
      headline="Reset your password."
      sub="Someone asked to reset your password. If that was you, choose a new one."
      chip="Password reset"
      feature={<GlassNote label="For the account" value={p.email} expiresIn={p.expiresIn} />}
      caption={{
        title: "Didn't ask for this?",
        body: "Ignore this email. Your password stays as it is, and the link works only once.",
      }}
      to={p.email}
    >
      <Action href={p.url}>Choose a new password</Action>
    </Layout>
  );
}
