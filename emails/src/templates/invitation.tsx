import { Action, GlassNote, Layout } from "../layout.tsx";

export const subject = "{{inviter}} invited you to {{organization}} on oryxa";

export default function Invitation(p: {
  assets: string;
  email: string;
  inviter: string;
  organization: string;
  url: string;
  expiresIn: string;
}) {
  return (
    <Layout
      assets={p.assets}
      title={`Join ${p.organization} on oryxa`}
      preview={`${p.inviter} invited you to work together on oryxa.`}
      scene="moon-earth-v2"
      headline={`Join ${p.organization} on oryxa.`}
      sub="On oryxa, a teammate's agent answers for them, only after they say yes."
      chip="Invitation"
      feature={
        <GlassNote
          label={`${p.inviter} invited you to`}
          value={p.organization}
          expiresIn={p.expiresIn}
        />
      }
      caption={{
        title: "Weren't expecting this?",
        body: "You can ignore this email. Nothing happens unless you accept.",
      }}
      to={p.email}
    >
      <Action href={p.url}>Accept the invitation</Action>
    </Layout>
  );
}
