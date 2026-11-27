import { GlassCode, Layout } from "../layout.tsx";

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
      title="Confirm your email"
      preview="One code, and your account is ready."
      scene="moon-rise-v2"
      headline="Confirm your email."
      sub={
        <>
          Enter this code to confirm <span style={{ fontWeight: 600 }}>{p.email}</span> and finish
          setting up your account.
        </>
      }
      chip="Confirmation code"
      feature={<GlassCode code={p.code} expiresIn={p.expiresIn} />}
      caption={{
        title: "Didn't create an account?",
        body: "You can ignore this email. Nothing is set up without the code.",
      }}
      to={p.email}
    />
  );
}
