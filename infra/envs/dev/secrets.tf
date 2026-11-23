# Containers only: the owner sets each value with the AWS CLI (docs/deploy.md), so no secret is
# ever in tofu state. Named by the task that reads it, so each role can be scoped to its own.
locals {
  secrets = {
    "core/database-url"          = "Postgres URL as the app role (core/sql/app_role.sql)"
    "core/workos-api-key"        = "WorkOS API key"
    "core/workos-client-id"      = "WorkOS client id"
    "core/assertion-private-key" = "Ed25519 PKCS#8 PEM that signs assertions"
    "core/event-signing-key"     = "Ed25519 PKCS#8 PEM that signs mandate.core events"
    "core/event-public-key"      = "Ed25519 SPKI PEM of mandate.core, for the core's own consumers"
    "migrate/database-url"       = "Postgres URL as the migration role (the instance's master user)"
    "eve/database-url"           = "Postgres URL as eve's own role, to its own database (eve)"
    "eve/ai-gateway-key"         = "AI gateway key"
    "eve/assertion-public-key"   = "Ed25519 SPKI PEM eve's route auth verifies assertions with"
  }
}

resource "aws_secretsmanager_secret" "this" {
  for_each    = local.secrets
  name        = "${local.name}/${each.key}"
  description = each.value
  kms_key_id  = aws_kms_key.main.arn
}
