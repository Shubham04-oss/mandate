# Alarms to one SNS topic. No subscription unless alarm_email is set (it must be confirmed by email).
# The topic uses no KMS key: CloudWatch cannot publish to one under the AWS-managed SNS key, and
# alarm notices carry only resource names and metric values.
resource "aws_sns_topic" "alarms" {
  name = "${local.name}-alarms"
}

resource "aws_sns_topic_subscription" "alarm_email" {
  count     = var.alarm_email == null ? 0 : 1
  topic_arn = aws_sns_topic.alarms.arn
  protocol  = "email"
  endpoint  = var.alarm_email
}

# Anything in a dead-letter queue is an event a consumer gave up on.
resource "aws_cloudwatch_metric_alarm" "dlq" {
  for_each            = aws_sqs_queue.dead_letter
  alarm_name          = "${local.name}-${each.key}-dlq-not-empty"
  alarm_description   = "Messages in ${each.value.name}: a consumer is failing events"
  namespace           = "AWS/SQS"
  metric_name         = "ApproximateNumberOfMessagesVisible"
  dimensions          = { QueueName = each.value.name }
  statistic           = "Maximum"
  period              = 300
  evaluation_periods  = 1
  comparison_operator = "GreaterThanThreshold"
  threshold           = 0
  # SQS stops reporting for an idle queue; an idle DLQ is the healthy state.
  treat_missing_data = "notBreaching"
  alarm_actions      = [aws_sns_topic.alarms.arn]
  ok_actions         = [aws_sns_topic.alarms.arn]
}

# The same for SES bounces and complaints the core gave up on (email.tf).
resource "aws_cloudwatch_metric_alarm" "email_feedback_dlq" {
  count               = var.email_enabled ? 1 : 0
  alarm_name          = "${local.name}-email-feedback-dlq-not-empty"
  alarm_description   = "Messages in ${aws_sqs_queue.email_feedback_dead_letter[0].name}: the core is failing bounce or complaint notices"
  namespace           = "AWS/SQS"
  metric_name         = "ApproximateNumberOfMessagesVisible"
  dimensions          = { QueueName = aws_sqs_queue.email_feedback_dead_letter[0].name }
  statistic           = "Maximum"
  period              = 300
  evaluation_periods  = 1
  comparison_operator = "GreaterThanThreshold"
  threshold           = 0
  # SQS stops reporting for an idle queue; an idle DLQ is the healthy state.
  treat_missing_data = "notBreaching"
  alarm_actions      = [aws_sns_topic.alarms.arn]
  ok_actions         = [aws_sns_topic.alarms.arn]
}

# No running task. Container Insights (RunningTaskCount) is off on the cluster, so this counts the
# free AWS/ECS CPUUtilization datapoints: each running task reports one a minute.
resource "aws_cloudwatch_metric_alarm" "service_down" {
  for_each            = { core = aws_ecs_service.core.name, eve = aws_ecs_service.eve.name }
  alarm_name          = "${local.name}-${each.key}-no-running-task"
  alarm_description   = "ECS service ${each.value} has no running task"
  namespace           = "AWS/ECS"
  metric_name         = "CPUUtilization"
  dimensions          = { ClusterName = aws_ecs_cluster.main.name, ServiceName = each.value }
  statistic           = "SampleCount"
  period              = 60
  evaluation_periods  = 5
  datapoints_to_alarm = 5
  comparison_operator = "LessThanThreshold"
  threshold           = 1
  # No datapoints is exactly what a stopped service looks like.
  treat_missing_data = "breaching"
  alarm_actions      = [aws_sns_topic.alarms.arn]
  ok_actions         = [aws_sns_topic.alarms.arn]
}

# Below 15% of the allocated storage. Storage autoscaling grows the volume at 10% free, so this fires
# first and clears once it grows.
resource "aws_cloudwatch_metric_alarm" "db_free_storage" {
  alarm_name          = "${local.name}-db-free-storage-low"
  alarm_description   = "${aws_db_instance.main.identifier} has under 15% of its allocated storage free"
  namespace           = "AWS/RDS"
  metric_name         = "FreeStorageSpace"
  dimensions          = { DBInstanceIdentifier = aws_db_instance.main.identifier }
  statistic           = "Minimum"
  period              = 300
  evaluation_periods  = 2
  comparison_operator = "LessThanThreshold"
  threshold           = aws_db_instance.main.allocated_storage * 0.15 * 1024 * 1024 * 1024
  # RDS always reports; a gap is a metrics hiccup, not a full disk.
  treat_missing_data = "missing"
  alarm_actions      = [aws_sns_topic.alarms.arn]
  ok_actions         = [aws_sns_topic.alarms.arn]
}
