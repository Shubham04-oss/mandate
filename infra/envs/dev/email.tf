# The core's own email through SES (variables.tf email_enabled). The domain identity, with DKIM and
# MAIL FROM, is verified outside tofu (docs/deploy.md, Email), so it is referenced here, never owned.
locals {
  ses_identity_arn = "arn:aws:ses:${var.region}:${data.aws_caller_identity.current.account_id}:identity/${var.domain}"
}

resource "aws_sesv2_configuration_set" "main" {
  count                  = var.email_enabled ? 1 : 0
  configuration_set_name = local.name
  # Approval links travel in these emails: a server without TLS gets nothing rather than plaintext.
  delivery_options {
    tls_policy = "REQUIRE"
  }
  reputation_options {
    reputation_metrics_enabled = true
  }
  sending_options {
    sending_enabled = true
  }
}

# SES publishes only to the account's default bus, not to the core's eventsv2 bus.
resource "aws_sesv2_configuration_set_event_destination" "feedback" {
  count                  = var.email_enabled ? 1 : 0
  configuration_set_name = aws_sesv2_configuration_set.main[0].configuration_set_name
  event_destination_name = "feedback"
  event_destination {
    enabled              = true
    matching_event_types = ["BOUNCE", "COMPLAINT"]
    event_bridge_destination {
      event_bus_arn = "arn:aws:events:${var.region}:${data.aws_caller_identity.current.account_id}:event-bus/default"
    }
  }
}

# Detail-types and the tag path are from the SES guide's EventBridge schema (monitoring-eventbridge);
# the tag keeps other configuration sets' notices off this queue.
resource "aws_cloudwatch_event_rule" "email_feedback" {
  count          = var.email_enabled ? 1 : 0
  name           = "${local.name}-email-feedback"
  description    = "SES bounces and complaints for the ${local.name} configuration set"
  event_bus_name = "default"
  event_pattern = jsonencode({
    source        = ["aws.ses"]
    "detail-type" = ["Email Bounced", "Email Complaint Received"]
    detail        = { mail = { tags = { "ses:configuration-set" = [aws_sesv2_configuration_set.main[0].configuration_set_name] } } }
  })
}

resource "aws_cloudwatch_event_target" "email_feedback" {
  count          = var.email_enabled ? 1 : 0
  rule           = aws_cloudwatch_event_rule.email_feedback[0].name
  event_bus_name = aws_cloudwatch_event_rule.email_feedback[0].event_bus_name
  arn            = aws_sqs_queue.email_feedback[0].arn
}

resource "aws_sqs_queue" "email_feedback_dead_letter" {
  count                     = var.email_enabled ? 1 : 0
  name                      = "${local.name}-email-feedback-dlq"
  sqs_managed_sse_enabled   = true
  message_retention_seconds = 1209600
}

# Standard, not FIFO: SES delivers these best-effort and unordered, and the core handles each alone.
# SSE-SQS, not the CMK, so EventBridge can send without a key grant.
resource "aws_sqs_queue" "email_feedback" {
  count                      = var.email_enabled ? 1 : 0
  name                       = "${local.name}-email-feedback"
  sqs_managed_sse_enabled    = true
  visibility_timeout_seconds = 60
  redrive_policy = jsonencode({
    deadLetterTargetArn = aws_sqs_queue.email_feedback_dead_letter[0].arn
    maxReceiveCount     = 5
  })
}

# Only this rule may send, so nothing else in the account can forge a bounce.
resource "aws_sqs_queue_policy" "email_feedback" {
  count     = var.email_enabled ? 1 : 0
  queue_url = aws_sqs_queue.email_feedback[0].id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "events.amazonaws.com" }
      Action    = "sqs:SendMessage"
      Resource  = aws_sqs_queue.email_feedback[0].arn
      Condition = { ArnEquals = { "aws:SourceArn" = aws_cloudwatch_event_rule.email_feedback[0].arn } }
    }]
  })
}

# The core sends as the domain through this configuration set only, and reads its feedback queue.
# The email consumer queue is covered by core_events (events.tf).
resource "aws_iam_role_policy" "core_email" {
  count = var.email_enabled ? 1 : 0
  name  = "email"
  role  = aws_iam_role.core_task.name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = "ses:SendEmail"
        Resource = [local.ses_identity_arn, aws_sesv2_configuration_set.main[0].arn]
      },
      {
        Effect   = "Allow"
        Action   = ["sqs:ReceiveMessage", "sqs:DeleteMessage", "sqs:ChangeMessageVisibility", "sqs:GetQueueAttributes"]
        Resource = aws_sqs_queue.email_feedback[0].arn
      },
    ]
  })
}
