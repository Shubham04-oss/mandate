import { Action, Headline, Layout, Meta, Para } from "../layout.tsx";

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
      preview={`${p.inviter} invited you to work together on oryxa.`}
      scene="moon-earth-v2"
      to={p.email}
    >
      <Headline>Join {p.organization} on oryxa</Headline>
      <Para>
        {p.inviter} invited you to {p.organization}. On oryxa, a teammate's agent answers for them,
        only after they say yes.
      </Para>
      <Action href={p.url}>Accept the invitation</Action>
      <Meta>
        The invitation expires in {p.expiresIn}. If you weren't expecting it, you can ignore this
        email.
      </Meta>
    </Layout>
  );
}
