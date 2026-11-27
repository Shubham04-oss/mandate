import { Action, Headline, Layout, Meta, Para } from "../layout.tsx";

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
      preview="Choose a new password for your account."
      scene="moon-rille"
      to={p.email}
    >
      <Headline>Reset your password</Headline>
      <Para>
        Someone asked to reset the password for {p.email}. If that was you, choose a new one.
      </Para>
      <Action href={p.url}>Choose a new password</Action>
      <Meta>
        The link works once and expires in {p.expiresIn}. If you didn't ask, ignore this email: your
        password stays as it is.
      </Meta>
    </Layout>
  );
}
