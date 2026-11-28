# Channels (variables.tf channels_enabled; docs/channels.md 13). The consumers are in events.tf, the
# secrets in secrets.tf, the env in ecs.tf. Here: the core's credential store, where it keeps each
# installation's provider token at runtime (CredentialStore in core/src/channels/credentials.rs).
locals {
  credential_prefix     = "${local.name}/channels/"
  credential_secret_arn = "arn:aws:secretsmanager:${var.region}:${data.aws_caller_identity.current.account_id}:secret:${local.credential_prefix}*"
}

# Only names under mandate-<env>/channels/, only under the environment's key: no other secret is
# reachable, and a token can never be stored under the AWS-managed key.
resource "aws_iam_role_policy" "core_channels" {
  count = var.channels_enabled ? 1 : 0
  name  = "channels"
  role  = aws_iam_role.core_task.name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect    = "Allow"
        Action    = "secretsmanager:CreateSecret"
        Resource  = local.credential_secret_arn
        Condition = { StringEquals = { "secretsmanager:KmsKeyArn" = aws_kms_key.main.arn } }
      },
      {
        Effect   = "Allow"
        Action   = ["secretsmanager:PutSecretValue", "secretsmanager:GetSecretValue", "secretsmanager:DeleteSecret"]
        Resource = local.credential_secret_arn
      },
      {
        Effect   = "Allow"
        Action   = ["kms:GenerateDataKey", "kms:Decrypt"]
        Resource = aws_kms_key.main.arn
        # Secrets Manager puts the secret's ARN in the encryption context, so the key serves these secrets only.
        Condition = {
          StringEquals = { "kms:ViaService" = "secretsmanager.${var.region}.amazonaws.com" }
          StringLike   = { "kms:EncryptionContext:SecretARN" = local.credential_secret_arn }
        }
      },
    ]
  })
  lifecycle {
    precondition {
      condition     = var.slack_client_id != ""
      error_message = "channels_enabled needs slack_client_id (docs/deploy.md, Channels)"
    }
  }
}
