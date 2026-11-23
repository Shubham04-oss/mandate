# The EventBridge enhanced custom bus (spec 6a). Adding a consumer is one entry here: a FIFO
# subscriber, its SQS FIFO queue and a dead-letter queue.
locals {
  consumers = {
    "agent-resume" = ["approval.decided.v1"]
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
  filter_configuration = {
    filters = [{
      scope   = "DATA"
      pattern = jsonencode({ type = each.value })
    }]
  }
  invoke_configuration = {
    target_arn = aws_sqs_queue.consumer[each.key].arn
    role_arn   = aws_iam_role.subscriber.arn
    # The envelope's tenant orders delivery per organization; its id deduplicates (spec 6a).
    sqs_parameters = {
      message_group_id         = "{% $events.data.tenant %}"
      message_deduplication_id = "{% $events.data.id %}"
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
