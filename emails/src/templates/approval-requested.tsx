import { Action, Ember, GlassNote, Layout, Ledger } from "../layout.tsx";

export const subject = "{{agent}} is waiting for your yes";

export default function ApprovalRequested(p: {
  assets: string;
  email: string;
  requester: string;
  agent: string;
  /** The agent inside a sentence: the same name, or "your agent" where `agent` is "Your agent". */
  agentInline: string;
  organization: string;
  url: string;
  expiresIn: string;
}) {
  return (
    <Layout
      assets={p.assets}
      title={`${p.agent} is waiting for your yes`}
      preview={`${p.requester} asked ${p.agentInline} to do something that needs your approval.`}
      scene="moon-dawn-v2"
      headline={`${p.agent} is waiting for your yes.`}
      sub={`${p.requester} asked ${p.agentInline} to do something that needs your approval. Nothing happens until you decide.`}
      chip={
        <>
          <Ember />
          Needs you
        </>
      }
      // Generic on purpose: what the agent wants to do shows only after sign-in (lock screens, previews).
      feature={
        <GlassNote label={`${p.agent} wants to`} value="Take an action" expiresIn={p.expiresIn} />
      }
      caption={{
        title: "Why you're getting this",
        body: `You approve requests for ${p.agentInline} in ${p.organization}.`,
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
