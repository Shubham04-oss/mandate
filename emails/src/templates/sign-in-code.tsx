import { Code, Headline, Layout, Meta, NightOnly, Para } from "../layout.tsx";

export const subject = "Your oryxa sign-in code";

export default function SignInCode(p: {
  assets: string;
  email: string;
  code: string;
  expiresIn: string;
}) {
  return (
    <Layout
      assets={p.assets}
      preview="Enter it on the sign-in page to continue."
      scene="moon-night-nebula-v2"
      to={p.email}
      feature={<Code value={p.code} />}
    >
      <Headline>Your sign-in code</Headline>
      <Para>Enter this code on the sign-in page to continue as {p.email}.</Para>
      <NightOnly>
        <Code value={p.code} />
      </NightOnly>
      <Meta>
        It expires in {p.expiresIn}. If you didn't try to sign in, you can ignore this email: nobody
        gets in without the code.
      </Meta>
    </Layout>
  );
}
