# The EventBridge enhanced custom bus (spec 6a). Adding a consumer is one entry here: a FIFO
# subscriber, its SQS FIFO queue and a dead-letter queue.
locals {
  consumers = {
    "agent-resume" = ["approval.decided.v1"]
    # Ids-only product events to PostHog's /batch. Always here, so the core starts whenever the key appears.
    "posthog" = [
      "approval.requested.v1", "approval.decided.v1", "conversation.started.v1", "conversation.finished.v1",
      "grant.created.v1", "grant.revoked.v1", "member.created.v1", "member.updated.v1", "member.removed.v1",
      "organization.created.v1", "member.signed_up.v1", "member.signed_in.v1", "organization.chosen.v1",
      "setting.changed.v1",
    ]
  }
}

resource "awscc_eventsv2_event_bus" "main" {
  name        = local.name
  description = "Mandate core events; app events join later as new sources"
  # Envelopes carry ids only, so the AWS-owned key is enough for dev.
  storage_configuration = {
    retention_period_in_days = 7
  }
}

resource "aws_sqs_queue" "dead_letter" {
  for_each                  = local.consumers
  name                      = "${local.name}-${each.key}-dlq.fifo"
  fifo_queue                = true
  sqs_managed_sse_enabled   = true
  message_retention_seconds = 1209600
}

resource "aws_sqs_queue" "consumer" {
  for_each                    = local.consumers
  name                        = "${local.name}-${each.key}.fifo"
  fifo_queue                  = true
  content_based_deduplication = false
  sqs_managed_sse_enabled     = true
  visibility_timeout_seconds  = 60
  redrive_policy = jsonencode({
    deadLetterTargetArn = aws_sqs_queue.dead_letter[each.key].arn
    maxReceiveCount     = 5
  })
}

resource "aws_iam_role" "subscriber" {
  name = "${local.name}-event-delivery"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "events.amazonaws.com" }
      Action    = "sts:AssumeRole"
      Condition = { StringEquals = { "aws:SourceAccount" = data.aws_caller_identity.current.account_id } }
    }]
  })
}

resource "aws_iam_role_policy" "subscriber" {
  name = "deliver"
  role = aws_iam_role.subscriber.name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect   = "Allow"
      Action   = "sqs:SendMessage"
      Resource = concat(values(aws_sqs_queue.consumer)[*].arn, values(aws_sqs_queue.dead_letter)[*].arn)
    }]
  })
}

resource "awscc_eventsv2_subscriber" "consumer" {
  for_each      = local.consumers
  name          = each.key
  event_bus_arn = awscc_eventsv2_event_bus.main.event_bus_arn
  type          = "FIFO"
  # For PutEvents the DATA scope is EventBridge's envelope with our payload under `detail`, so a
  # top-level `type` never matched. aws:DetailType comes from the entry and publishers can't forge it.
  filter_configuration = {
    filters = [{
      scope   = "SYSTEM_METADATA"
      pattern = jsonencode({ "aws:DetailType" = each.value })
    }]
  }
  invoke_configuration = {
    target_arn = aws_sqs_queue.consumer[each.key].arn
    role_arn   = aws_iam_role.subscriber.arn
    # The core publishes with EventGroupId = tenant and DeduplicationId = event id (spec 6a).
    sqs_parameters = {
      message_group_id         = "{% $events.SystemMetadata.EventGroupId %}"
      message_deduplication_id = "{% $events.SystemMetadata.DeduplicationId %}"
    }
  }
  transformer = {
    type = "RAW"
  }
  retry_policy = {
    max_event_age_in_seconds = 86400
  }
  on_failure_configuration = {
    arn = aws_sqs_queue.dead_letter[each.key].arn
  }
  depends_on = [aws_iam_role_policy.subscriber]
}

resource "aws_iam_role_policy" "core_events" {
  name = "events"
  role = aws_iam_role.core_task.name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = "events:PutEvents"
        Resource = awscc_eventsv2_event_bus.main.event_bus_arn
      },
      {
        Effect   = "Allow"
        Action   = ["sqs:ReceiveMessage", "sqs:DeleteMessage", "sqs:ChangeMessageVisibility", "sqs:GetQueueAttributes"]
        Resource = values(aws_sqs_queue.consumer)[*].arn
      },
    ]
  })
}
