import { Code, Headline, Layout, Meta, Para } from "../layout.tsx";

export const subject = "Confirm your email for oryxa";

export default function VerifyEmail(p: {
  assets: string;
  email: string;
  code: string;
  expiresIn: string;
}) {
  return (
    <Layout
      assets={p.assets}
      preview="One code, and your account is ready."
      scene="moon-rise-v2"
      to={p.email}
    >
      <Headline>Confirm your email</Headline>
      <Para>Enter this code to confirm {p.email} and finish setting up your account.</Para>
      <Code value={p.code} />
      <Meta>
        It expires in {p.expiresIn}. If you didn't create an oryxa account, you can ignore this
        email.
      </Meta>
    </Layout>
  );
}
