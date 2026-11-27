import { Action, Ember, GlassNote, Layout, Ledger } from "../layout.tsx";

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
      title={`${p.agent} is waiting for your yes`}
      preview={`${p.requester} asked ${p.agent} to do something that needs your approval.`}
      scene="moon-dawn-v2"
      headline={`${p.agent} is waiting for your yes.`}
      sub={`${p.requester} asked ${p.agent} to do something that needs your approval. Nothing happens until you decide.`}
      chip={
        <>
          <Ember />
          Needs you
        </>
      }
      feature={<GlassNote label={`${p.agent} wants to`} value={p.tool} expiresIn={p.expiresIn} />}
      caption={{
        title: "Why you're getting this",
        body: `You approve requests for ${p.agent} in ${p.organization}.`,
      }}
      to={p.email}
    >
      <Ledger
        rows={[
          ["Asked by", p.requester],
          ["Agent", p.agent],
          ["Organization", p.organization],
        ]}
      />
      <Action href={p.url}>Review the request</Action>
    </Layout>
  );
}
