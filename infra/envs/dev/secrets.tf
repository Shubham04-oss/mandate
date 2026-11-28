# Containers only: the owner sets each value with the AWS CLI (docs/deploy.md), so none of these
# is in tofu state. Named by the task that reads it, so each role can be scoped to its own.
locals {
  secrets = {
    "core/database-url"          = "Postgres URL as the app role (core/sql/app_role.sql)"
    "core/workos-api-key"        = "WorkOS API key"
    "core/workos-client-id"      = "WorkOS client id"
    "core/assertion-private-key" = "Ed25519 PKCS#8 PEM that signs assertions"
    "core/event-signing-key"     = "Ed25519 PKCS#8 PEM that signs mandate.core events"
    "core/event-public-key"      = "Ed25519 SPKI PEM of mandate.core, for the core's own consumers"
    "eve/database-url"           = "Postgres URL as eve's own role, to its own database (eve)"
    "eve/ai-gateway-key"         = "AI gateway key"
    "eve/assertion-public-key"   = "Ed25519 SPKI PEM eve's route auth verifies assertions with"
    # Read by two tasks (the collector and the core), so named by neither; only telemetry_enabled wires it.
    "posthog-key" = "PostHog project API key (phc_...): OTLP export from the collector, events from the core and eve"
    # Channels (docs/channels.md 13): wired only while channels_enabled is on (and Twilio's only with
    # twilio_account_sid set). Per-installation bot tokens are created by the core at runtime under
    # mandate-<env>/channels/, never here.
    "core/slack-client-secret"  = "Slack app client secret (OAuth install and Sign in with Slack)"
    "core/slack-signing-secret" = "Slack app signing secret: verifies Slack's webhooks at the gateway"
    "core/decision-key"         = "32 random bytes, base64url: HMAC key of approval card decision tokens"
    "core/twilio-auth-token"    = "Twilio auth token: verifies Twilio's webhooks and calls Twilio's API"
    "channel-proxy-key"         = "32 random bytes, base64url: eve's key to the core's channel proxy (core and eve)"
    "twilio-resign-key"         = "32 random bytes, base64url: the core re-signs Twilio webhooks it forwards to eve (core and eve)"
  }
}

resource "aws_secretsmanager_secret" "this" {
  for_each    = local.secrets
  name        = "${local.name}/${each.key}"
  description = each.value
  kms_key_id  = aws_kms_key.main.arn
}

# The exception: tofu creates the tunnel, so its token is in state anyway (like the tunnel secret).
# Only cloudflared reads it; the core container never sees it.
resource "aws_secretsmanager_secret" "tunnel_token" {
  name        = "${local.name}/cloudflared/tunnel-token"
  description = "Cloudflare Tunnel token for the cloudflared sidecar"
  kms_key_id  = aws_kms_key.main.arn
}

resource "aws_secretsmanager_secret_version" "tunnel_token" {
  secret_id     = aws_secretsmanager_secret.tunnel_token.id
  secret_string = data.cloudflare_zero_trust_tunnel_cloudflared_token.api.token
}
