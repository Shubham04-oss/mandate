// The model eve talks to: Cloudflare AI Gateway's OpenAI-compatible endpoint, all of it from env.
import { createOpenAICompatible } from "@ai-sdk/openai-compatible";

/** `model` through the gateway at `url` (its `/compat` base), authenticated with a gateway token. */
export function gatewayModel(url: string, token: string, model: string, fetchFn?: typeof fetch) {
  return createOpenAICompatible({
    name: "gateway",
    baseURL: url,
    // No apiKey, so no Authorization header: the gateway holds the provider keys (BYOK).
    headers: { "cf-aig-authorization": `Bearer ${token}` },
    // Streamed usage feeds eve's session limits and compaction.
    includeUsage: true,
    fetch: fetchFn,
  })(model);
}
