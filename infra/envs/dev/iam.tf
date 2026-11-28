data "aws_caller_identity" "current" {}

locals {
  # Which secrets each task's execution role injects; eve and migrate never see the core's.
  # The core (product events, errors) and eve (errors, model calls) also get the PostHog project
  # key, only while telemetry is on. It is write-only and the browser ships it too (ORY-82).
  task_secrets = {
    for task in ["core", "eve", "migrate"] : task => {
      for k, s in aws_secretsmanager_secret.this : k => s.arn
      if(startswith(k, "${task}/") && lookup(local.secret_switch, k, true)) || contains(lookup(local.shared_secret_readers, k, []), task)
    }
  }
  # Secrets read by more than one task, so named by none: who reads each, only while its feature is on.
  shared_secret_readers = {
    "posthog-key"       = var.telemetry_enabled ? ["core", "eve"] : []
    "channel-proxy-key" = var.channels_enabled ? ["core", "eve"] : []
    "twilio-resign-key" = local.twilio_enabled ? ["core", "eve"] : []
  }
  # A task's own secrets that stay unwired until their feature is on, so an empty one never blocks a start.
  secret_switch = {
    "core/slack-client-secret"  = var.channels_enabled
    "core/slack-signing-secret" = var.channels_enabled
    "core/decision-key"         = var.channels_enabled
    "core/twilio-auth-token"    = local.twilio_enabled
  }
  twilio_enabled = var.channels_enabled && var.twilio_account_sid != ""
}

resource "aws_iam_role" "execution" {
  for_each = toset(["core", "eve", "migrate"])
  name     = "${local.name}-${each.key}-execution"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "ecs-tasks.amazonaws.com" }
      Action    = "sts:AssumeRole"
      Condition = { StringEquals = { "aws:SourceAccount" = data.aws_caller_identity.current.account_id } }
    }]
  })
}

# Pull one image, write one log group, read the task's own secrets; nothing else.
resource "aws_iam_role_policy" "execution" {
  for_each = toset(["core", "eve", "migrate"])
  name     = "execution"
  role     = aws_iam_role.execution[each.key].name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = "ecr:GetAuthorizationToken"
        Resource = "*"
      },
      {
        Effect   = "Allow"
        Action   = ["ecr:BatchGetImage", "ecr:GetDownloadUrlForLayer", "ecr:BatchCheckLayerAvailability"]
        Resource = aws_ecr_repository.this[each.key == "eve" ? "eve" : "core"].arn
      },
      {
        Effect   = "Allow"
        Action   = ["logs:CreateLogStream", "logs:PutLogEvents"]
        Resource = "${aws_cloudwatch_log_group.this[each.key].arn}:*"
      },
      {
        Effect   = "Allow"
        Action   = "secretsmanager:GetSecretValue"
        Resource = concat(values(local.task_secrets[each.key]), lookup(local.execution_extra_secrets, each.key, []))
      },
      {
        Effect    = "Allow"
        Action    = "kms:Decrypt"
        Resource  = aws_kms_key.main.arn
        Condition = { StringEquals = { "kms:ViaService" = "secretsmanager.${var.region}.amazonaws.com" } }
      },
    ]
  })
}

locals {
  db_master_secret_arn = one(aws_db_instance.main.master_user_secret[*].secret_arn)
  # migrate takes the master password live; the core's cloudflared sidecar takes the tunnel token.
  execution_extra_secrets = {
    migrate = [local.db_master_secret_arn]
    core    = [aws_secretsmanager_secret.tunnel_token.arn]
  }
}

# db-bootstrap pulls its mirrored image from the core repository and reads the master secret and the two URLs.
resource "aws_iam_role" "db_bootstrap_execution" {
  name = "${local.name}-db-bootstrap-execution"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "ecs-tasks.amazonaws.com" }
      Action    = "sts:AssumeRole"
      Condition = { StringEquals = { "aws:SourceAccount" = data.aws_caller_identity.current.account_id } }
    }]
  })
}

resource "aws_iam_role_policy" "db_bootstrap_execution" {
  name = "execution"
  role = aws_iam_role.db_bootstrap_execution.name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = "ecr:GetAuthorizationToken"
        Resource = "*"
      },
      {
        Effect   = "Allow"
        Action   = ["ecr:BatchGetImage", "ecr:GetDownloadUrlForLayer", "ecr:BatchCheckLayerAvailability"]
        Resource = aws_ecr_repository.this["core"].arn
      },
      {
        Effect   = "Allow"
        Action   = ["logs:CreateLogStream", "logs:PutLogEvents"]
        Resource = "${aws_cloudwatch_log_group.this["db-bootstrap"].arn}:*"
      },
      {
        Effect = "Allow"
        Action = "secretsmanager:GetSecretValue"
        Resource = [
          local.db_master_secret_arn,
          aws_secretsmanager_secret.this["core/database-url"].arn,
          aws_secretsmanager_secret.this["eve/database-url"].arn,
        ]
      },
      {
        Effect    = "Allow"
        Action    = "kms:Decrypt"
        Resource  = aws_kms_key.main.arn
        Condition = { StringEquals = { "kms:ViaService" = "secretsmanager.${var.region}.amazonaws.com" } }
      },
    ]
  })
}

# The collector pulls its public image from Docker Hub, so: one log group and the PostHog key.
resource "aws_iam_role" "collector_execution" {
  count = var.telemetry_enabled ? 1 : 0
  name  = "${local.name}-collector-execution"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "ecs-tasks.amazonaws.com" }
      Action    = "sts:AssumeRole"
      Condition = { StringEquals = { "aws:SourceAccount" = data.aws_caller_identity.current.account_id } }
    }]
  })
}

resource "aws_iam_role_policy" "collector_execution" {
  count = var.telemetry_enabled ? 1 : 0
  name  = "execution"
  role  = aws_iam_role.collector_execution[0].name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = ["logs:CreateLogStream", "logs:PutLogEvents"]
        Resource = "${aws_cloudwatch_log_group.this["collector"].arn}:*"
      },
      {
        Effect   = "Allow"
        Action   = "secretsmanager:GetSecretValue"
        Resource = [aws_secretsmanager_secret.this["posthog-key"].arn]
      },
      {
        Effect    = "Allow"
        Action    = "kms:Decrypt"
        Resource  = aws_kms_key.main.arn
        Condition = { StringEquals = { "kms:ViaService" = "secretsmanager.${var.region}.amazonaws.com" } }
      },
    ]
  })
}

# The core's own AWS permissions (bus, queue, browser, guardrail) are added beside what uses them.
resource "aws_iam_role" "core_task" {
  name = "${local.name}-core-task"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "ecs-tasks.amazonaws.com" }
      Action    = "sts:AssumeRole"
      Condition = { StringEquals = { "aws:SourceAccount" = data.aws_caller_identity.current.account_id } }
    }]
  })
}
