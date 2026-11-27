import {
  Action,
  GlassNote,
  Headline,
  Layout,
  Ledger,
  Meta,
  NeedsYou,
  NightOnly,
  Para,
} from "../layout.tsx";

export const subject = "{{agent}} is waiting for your yes";

export default function ApprovalRequested(p: {
  assets: string;
  email: string;
  requester: string;
  agent: string;
  tool: string;
  organization: string;
  url: string;
  expiresIn: string;
}) {
  return (
    <Layout
      assets={p.assets}
      preview={`${p.agent} is waiting for your decision.`}
      scene="moon-dawn-v2"
      to={p.email}
      feature={
        <>
          <NeedsYou />
          <GlassNote label={`${p.agent} wants to`} value={p.tool} />
        </>
      }
    >
      <NightOnly>
        <NeedsYou />
      </NightOnly>
      <Headline>{p.agent} is waiting for your yes</Headline>
      <Para>
        {p.requester} asked {p.agent} to do something that needs your approval. Nothing happens
        until you decide.
      </Para>
      <Ledger
        rows={[
          ["Asked by", p.requester],
          ["Agent", p.agent],
          ["Wants to", p.tool],
          ["Expires in", p.expiresIn],
        ]}
      />
      <Action href={p.url}>Review the request</Action>
      <Meta>
        You're getting this because you approve requests for {p.agent} in {p.organization}.
      </Meta>
    </Layout>
  );
}
