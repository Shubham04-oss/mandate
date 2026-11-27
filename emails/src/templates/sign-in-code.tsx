import { GlassCode, Layout } from "../layout.tsx";

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
      title="Your sign-in code"
      preview="Enter it on the sign-in page to continue."
      scene="moon-night-nebula-v2"
      headline="Your sign-in code is ready."
      sub={
        <>
          Enter it on the sign-in page to continue as{" "}
          <span style={{ fontWeight: 600 }}>{p.email}</span>.
        </>
      }
      chip="Sign-in code"
      feature={<GlassCode code={p.code} expiresIn={p.expiresIn} />}
      caption={{
        title: "Didn't try to sign in?",
        body: "You can ignore this email. Nobody gets in without the code.",
      }}
      to={p.email}
    />
  );
}
