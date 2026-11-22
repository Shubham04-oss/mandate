# Checks against mock providers (a mocked apply, so computed values are concrete): no AWS or
# Cloudflare account or credentials involved.
# One file, because mock providers cannot be shared between test files.
mock_provider "aws" {
  mock_data "aws_availability_zones" {
    defaults = { names = ["ap-south-1a", "ap-south-1b"] }
  }
  mock_data "aws_caller_identity" {
    defaults = { account_id = "000000000000" }
  }
  mock_resource "aws_iam_role" {
    defaults = { arn = "arn:aws:iam::000000000000:role/mock" }
  }
  mock_resource "aws_acm_certificate" {
    defaults = { arn = "arn:aws:acm:ap-south-1:000000000000:certificate/mock" }
  }
  mock_resource "aws_service_discovery_service" {
    defaults = { arn = "arn:aws:servicediscovery:ap-south-1:000000000000:service/srv-mock" }
  }
  mock_resource "aws_lb_target_group" {
    defaults = { arn = "arn:aws:elasticloadbalancing:ap-south-1:000000000000:targetgroup/mock/0" }
  }
  mock_resource "aws_lb_listener" {
    defaults = { arn = "arn:aws:elasticloadbalancing:ap-south-1:000000000000:listener/app/mock/0/0" }
  }
  mock_resource "aws_lb" {
    defaults = { arn = "arn:aws:elasticloadbalancing:ap-south-1:000000000000:loadbalancer/app/mock/0" }
  }
  mock_resource "aws_s3_bucket" {
    defaults = { bucket = "mandate-dev-recordings-mock", arn = "arn:aws:s3:::mandate-dev-recordings-mock" }
  }
  mock_resource "aws_bedrock_guardrail" {
    defaults = { guardrail_arn = "arn:aws:bedrock:ap-south-1:000000000000:guardrail/mock", guardrail_id = "mock" }
  }
  mock_resource "aws_iam_openid_connect_provider" {
    defaults = { arn = "arn:aws:iam::000000000000:oidc-provider/token.actions.githubusercontent.com" }
  }
  mock_resource "aws_ecs_cluster" {
    defaults = { arn = "arn:aws:ecs:ap-south-1:000000000000:cluster/mandate-dev" }
  }
  mock_resource "aws_kms_key" {
    defaults = { arn = "arn:aws:kms:ap-south-1:000000000000:key/mock" }
  }
}
mock_provider "awscc" {
  mock_resource "awscc_eventsv2_event_bus" {
    defaults = { event_bus_arn = "arn:aws:events:ap-south-1:000000000000:event-busv2/mandate-dev/abcdefghij0123456789abcde" }
  }
}
mock_provider "cloudflare" {
  mock_data "cloudflare_ip_ranges" {
    defaults = {
      ipv4_cidrs = ["173.245.48.0/20", "103.21.244.0/22"]
      ipv6_cidrs = ["2400:cb00::/32"]
    }
  }
}

# Mocks give every instance of a type the same values; distinct ids let the checks tell them apart.
override_resource {
  target = aws_secretsmanager_secret.this["core/database-url"]
  values = { arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/core/database-url" }
}
override_resource {
  target = aws_secretsmanager_secret.this["core/workos-api-key"]
  values = { arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/core/workos-api-key" }
}
override_resource {
  target = aws_secretsmanager_secret.this["core/workos-client-id"]
  values = { arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/core/workos-client-id" }
}
override_resource {
  target = aws_secretsmanager_secret.this["core/assertion-private-key"]
  values = { arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/core/assertion-private-key" }
}
override_resource {
  target = aws_secretsmanager_secret.this["core/event-signing-key"]
  values = { arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/core/event-signing-key" }
}
override_resource {
  target = aws_secretsmanager_secret.this["core/event-public-key"]
  values = { arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/core/event-public-key" }
}
override_resource {
  target = aws_secretsmanager_secret.this["migrate/database-url"]
  values = { arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/migrate/database-url" }
}
override_resource {
  target = aws_secretsmanager_secret.this["eve/database-url"]
  values = { arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/eve/database-url" }
}
override_resource {
  target = aws_secretsmanager_secret.this["eve/ai-gateway-key"]
  values = { arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/eve/ai-gateway-key" }
}
override_resource {
  target = aws_secretsmanager_secret.this["eve/assertion-public-key"]
  values = { arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/eve/assertion-public-key" }
}
override_resource {
  target = aws_security_group.alb
  values = { id = "sg-alb" }
}
override_resource {
  target = aws_security_group.core
  values = { id = "sg-core" }
}
override_resource {
  target = aws_security_group.eve
  values = { id = "sg-eve" }
}
override_resource {
  target = aws_security_group.migrate
  values = { id = "sg-migrate" }
}
override_resource {
  target = aws_security_group.db
  values = { id = "sg-db" }
}
override_resource {
  target = aws_security_group.endpoints
  values = { id = "sg-endpoints" }
}
override_resource {
  target = aws_sqs_queue.consumer["agent-resume"]
  values = { arn = "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-agent-resume.fifo" }
}
override_resource {
  target = aws_sqs_queue.dead_letter["agent-resume"]
  values = { arn = "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-agent-resume-dlq.fifo" }
}

override_resource {
  target = aws_iam_role.execution["core"]
  values = { arn = "arn:aws:iam::000000000000:role/mandate-dev-core-execution" }
}
override_resource {
  target = aws_iam_role.execution["eve"]
  values = { arn = "arn:aws:iam::000000000000:role/mandate-dev-eve-execution" }
}
override_resource {
  target = aws_iam_role.execution["migrate"]
  values = { arn = "arn:aws:iam::000000000000:role/mandate-dev-migrate-execution" }
}
override_resource {
  target = aws_iam_role.core_task
  values = { arn = "arn:aws:iam::000000000000:role/mandate-dev-core-task" }
}

variables {
  domain             = "example.test"
  cloudflare_zone_id = "0123456789abcdef0123456789abcdef"
  ai_gateway_url     = "https://gateway.example.test"
  model_default      = "model"
}

run "private_subnets_have_no_public_ips_and_route_through_nat" {
  command = apply

  assert {
    condition     = alltrue([for s in aws_subnet.private : !s.map_public_ip_on_launch])
    error_message = "private subnets must not hand out public IPs"
  }
  assert {
    condition     = length(aws_subnet.private) == 2 && length(aws_subnet.public) == 2
    error_message = "two AZs: Aurora and the ALB both need two subnets"
  }
  assert {
    condition     = aws_nat_gateway.egress.availability_mode == "regional"
    error_message = "one regional NAT gateway for egress"
  }
}

run "default_security_group_allows_nothing" {
  command = apply

  assert {
    condition     = length(aws_default_security_group.locked.ingress) == 0 && length(aws_default_security_group.locked.egress) == 0
    error_message = "the default security group must have no rules"
  }
}

run "endpoints_cover_what_tasks_need_without_nat" {
  command = apply

  assert {
    condition     = toset(keys(aws_vpc_endpoint.interface)) == toset(["ecr.api", "ecr.dkr", "logs", "secretsmanager"])
    error_message = "interface endpoints: ECR, logs, Secrets Manager"
  }
  assert {
    condition     = aws_vpc_endpoint.s3.vpc_endpoint_type == "Gateway"
    error_message = "S3 (ECR layers) through the free gateway endpoint"
  }
}

run "aurora_scales_to_zero_and_never_holds_a_password_in_state" {
  command = apply

  assert {
    condition     = aws_rds_cluster.main.serverlessv2_scaling_configuration[0].min_capacity == 0
    error_message = "dev Aurora scales to zero"
  }
  assert {
    condition     = aws_rds_cluster.main.manage_master_user_password == true && aws_rds_cluster.main.master_password == null
    error_message = "RDS manages the master password in Secrets Manager; it never enters state"
  }
  assert {
    condition     = aws_rds_cluster.main.storage_encrypted && aws_rds_cluster.main.deletion_protection
    error_message = "encrypted with the CMK and protected from deletion"
  }
  assert {
    condition     = alltrue([for i in aws_rds_cluster_instance.main : !i.publicly_accessible])
    error_message = "no public database instance"
  }
  assert {
    condition     = one([for p in aws_rds_cluster_parameter_group.main.parameter : p.value if p.name == "rds.force_ssl"]) == "1"
    error_message = "connections must use TLS"
  }
}

run "database_takes_no_traffic_from_addresses" {
  command = apply

  assert {
    condition     = length(aws_security_group.db.ingress) == 0
    error_message = "the database security group admits only other security groups, by rule"
  }
}

run "secrets_are_containers_under_the_cmk" {
  command = apply

  assert {
    condition = toset(keys(aws_secretsmanager_secret.this)) == toset([
      "core/database-url", "core/workos-api-key", "core/workos-client-id",
      "core/assertion-private-key", "core/event-signing-key", "core/event-public-key",
      "migrate/database-url", "eve/database-url", "eve/ai-gateway-key", "eve/assertion-public-key",
    ])
    error_message = "one secret per value a task needs, named by who reads it"
  }
  assert {
    condition     = aws_kms_key.main.enable_key_rotation
    error_message = "the CMK rotates"
  }
}

run "only_cloudflare_reaches_the_load_balancer" {
  command = apply

  assert {
    condition     = toset([for r in aws_vpc_security_group_ingress_rule.alb_from_cloudflare : r.cidr_ipv4]) == toset(["173.245.48.0/20", "103.21.244.0/22"])
    error_message = "the ALB admits Cloudflare's ranges and nothing else"
  }
  assert {
    condition     = alltrue([for r in aws_vpc_security_group_ingress_rule.alb_from_cloudflare : r.from_port == 443 && r.to_port == 443])
    error_message = "HTTPS only"
  }
  assert {
    condition     = aws_lb.api.drop_invalid_header_fields
    error_message = "drop malformed headers at the edge"
  }
}

run "the_edge_forwards_the_web_api_only" {
  command = apply

  assert {
    condition     = aws_lb_listener.https.default_action[0].type == "fixed-response" && aws_lb_listener.https.default_action[0].fixed_response[0].status_code == "404"
    error_message = "anything not listed is a 404 at the load balancer (the MCP server stays private)"
  }
  assert {
    condition     = toset(flatten([for c in aws_lb_listener_rule.api.condition : [for p in c.path_pattern : p.values]])) == toset(["/v1/*", "/healthz"])
    error_message = "forward only the web API and the health check"
  }
  assert {
    condition     = toset(flatten([for c in aws_lb_listener_rule.api.condition : [for h in c.host_header : h.values]])) == toset(["api.example.test"])
    error_message = "only the API host"
  }
}

run "eve_is_private_and_reachable_only_from_the_core" {
  command = apply

  assert {
    condition     = aws_vpc_security_group_ingress_rule.eve_from_core.referenced_security_group_id == aws_security_group.core.id
    error_message = "eve admits the core only"
  }
  assert {
    condition     = length(aws_ecs_service.eve.load_balancer) == 0 && !aws_ecs_service.eve.network_configuration[0].assign_public_ip
    error_message = "eve has no load balancer and no public IP"
  }
  assert {
    condition     = aws_ecs_task_definition.eve.task_role_arn == null
    error_message = "eve holds no AWS credentials at all"
  }
}

run "each_task_reads_only_its_own_secrets" {
  command = apply

  assert {
    condition = toset([for s in jsondecode(aws_ecs_task_definition.core.container_definitions)[0].secrets : s.valueFrom]) == toset([
      for k, s in aws_secretsmanager_secret.this : s.arn if startswith(k, "core/")
    ])
    error_message = "the core gets exactly the core secrets"
  }
  assert {
    condition = toset([for s in jsondecode(aws_ecs_task_definition.eve.container_definitions)[0].secrets : s.valueFrom]) == toset([
      for k, s in aws_secretsmanager_secret.this : s.arn if startswith(k, "eve/")
    ])
    error_message = "eve gets exactly the eve secrets"
  }
  assert {
    condition = toset(flatten([
      for st in jsondecode(aws_iam_role_policy.execution["eve"].policy).Statement : st.Resource if contains(flatten([st.Action]), "secretsmanager:GetSecretValue")
      ])) == toset([
      for k, s in aws_secretsmanager_secret.this : s.arn if startswith(k, "eve/")
    ])
    error_message = "eve's execution role can read only eve's secrets"
  }
  assert {
    condition = toset(flatten([
      for st in jsondecode(aws_iam_role_policy.execution["migrate"].policy).Statement : st.Resource if contains(flatten([st.Action]), "secretsmanager:GetSecretValue")
    ])) == toset([aws_secretsmanager_secret.this["migrate/database-url"].arn])
    error_message = "the migrate task reads only the migration URL"
  }
  assert {
    condition = toset([for s in jsondecode(aws_ecs_task_definition.core.container_definitions)[0].secrets : s.name]) == toset([
      "DATABASE_URL", "WORKOS_API_KEY", "WORKOS_CLIENT_ID", "MANDATE_ASSERTION_PRIVATE_KEY_PEM",
      "MANDATE_EVENT_SIGNING_KEY_PEM", "MANDATE_EVENT_PUBLIC_KEY_PEM",
    ])
    error_message = "the core signs events and verifies its own (lane F's MANDATE_EVENT_PUBLIC_KEYS)"
  }
  assert {
    condition = toset([for s in jsondecode(aws_ecs_task_definition.eve.container_definitions)[0].secrets : s.name]) == toset([
      "WORKFLOW_POSTGRES_URL", "MANDATE_AI_GATEWAY_KEY", "MANDATE_ASSERTION_PUBLIC_KEY",
    ])
    error_message = "eve's secrets under the names lane D's agents/ reads"
  }
  assert {
    condition     = !strcontains(aws_ecs_task_definition.core.container_definitions, "MANDATE_ENV")
    error_message = "dev-assert stays disabled in the deployed core"
  }
}

run "services_take_no_public_ip_and_images_are_immutable" {
  command = apply

  assert {
    condition     = !aws_ecs_service.core.network_configuration[0].assign_public_ip
    error_message = "the core is reached through the ALB only"
  }
  assert {
    condition     = alltrue([for r in aws_ecr_repository.this : r.image_tag_mutability == "IMMUTABLE"])
    error_message = "a deployed tag always names the same image"
  }
}

run "api_is_proxied_and_auth_is_rate_limited_at_the_edge" {
  command = apply

  assert {
    condition     = cloudflare_dns_record.api.proxied
    error_message = "the API is proxied by Cloudflare"
  }
  assert {
    condition     = cloudflare_ruleset.auth_rate_limit.phase == "http_ratelimit" && strcontains(cloudflare_ruleset.auth_rate_limit.rules[0].expression, "/v1/auth/")
    error_message = "WAF rate limit on /v1/auth/*"
  }
}

run "the_database_admits_only_the_tasks" {
  command = apply

  assert {
    condition = toset([for r in aws_vpc_security_group_ingress_rule.db : r.referenced_security_group_id]) == toset([
      aws_security_group.core.id, aws_security_group.eve.id, aws_security_group.migrate.id,
    ])
    error_message = "Postgres from the core, eve and migrate only"
  }
}

run "each_consumer_has_a_fifo_queue_with_a_dead_letter_queue" {
  command = apply

  assert {
    condition     = keys(aws_sqs_queue.consumer) == ["agent-resume"] && alltrue([for q in aws_sqs_queue.consumer : q.fifo_queue && q.sqs_managed_sse_enabled])
    error_message = "one encrypted FIFO queue per consumer, starting with agent-resume"
  }
  assert {
    condition     = alltrue([for k, q in aws_sqs_queue.consumer : jsondecode(q.redrive_policy).deadLetterTargetArn == aws_sqs_queue.dead_letter[k].arn])
    error_message = "every consumer queue redrives to its own dead-letter queue"
  }
  assert {
    condition     = alltrue([for s in awscc_eventsv2_subscriber.consumer : s.type == "FIFO" && strcontains(s.invoke_configuration.sqs_parameters.message_group_id, "tenant")])
    error_message = "FIFO delivery grouped by tenant, so each organization's events stay in order"
  }
  assert {
    condition     = jsondecode(awscc_eventsv2_subscriber.consumer["agent-resume"].filter_configuration.filters[0].pattern).type == ["approval.decided.v1"]
    error_message = "agent-resume receives approval.decided.v1 only"
  }
}

run "the_core_may_publish_and_consume_and_nothing_more" {
  command = apply

  assert {
    condition = jsondecode(aws_iam_role_policy.core_events.policy).Statement == [
      {
        Effect   = "Allow"
        Action   = "events:PutEvents"
        Resource = awscc_eventsv2_event_bus.main.event_bus_arn
      },
      {
        Effect   = "Allow"
        Action   = ["sqs:ReceiveMessage", "sqs:DeleteMessage", "sqs:ChangeMessageVisibility", "sqs:GetQueueAttributes"]
        Resource = [aws_sqs_queue.consumer["agent-resume"].arn]
      },
    ]
    error_message = "the core publishes to its bus and reads its consumer queues; nothing else"
  }
  assert {
    condition     = jsondecode(aws_iam_role.subscriber.assume_role_policy).Statement[0].Principal.Service == "events.amazonaws.com"
    error_message = "only EventBridge assumes the delivery role"
  }
  assert {
    condition     = jsondecode(aws_iam_role_policy.subscriber.policy).Statement[0].Resource == concat(values(aws_sqs_queue.consumer)[*].arn, values(aws_sqs_queue.dead_letter)[*].arn)
    error_message = "the delivery role sends only to the consumer queues and their dead letters"
  }
}

run "the_browser_is_public_mode_and_records_to_a_private_bucket" {
  command = apply

  assert {
    condition     = aws_bedrockagentcore_browser.main.network_configuration[0].network_mode == "PUBLIC"
    error_message = "the core requires a PUBLIC network mode browser (docs/security.md)"
  }
  assert {
    condition     = aws_bedrockagentcore_browser.main.recording[0].enabled && aws_bedrockagentcore_browser.main.recording[0].s3_location[0].bucket == aws_s3_bucket.recordings.bucket
    error_message = "every session records to the recordings bucket"
  }
  assert {
    condition = alltrue([
      aws_s3_bucket_public_access_block.recordings.block_public_acls,
      aws_s3_bucket_public_access_block.recordings.block_public_policy,
      aws_s3_bucket_public_access_block.recordings.ignore_public_acls,
      aws_s3_bucket_public_access_block.recordings.restrict_public_buckets,
    ])
    error_message = "recordings are never public"
  }
  assert {
    condition     = jsondecode(aws_s3_bucket_policy.recordings.policy).Statement[0].Condition.Bool["aws:SecureTransport"] == "false" && jsondecode(aws_s3_bucket_policy.recordings.policy).Statement[0].Effect == "Deny"
    error_message = "the bucket refuses plain HTTP"
  }
}

run "the_guardrail_blocks_credentials" {
  command = apply

  assert {
    condition = toset([
      for p in aws_bedrock_guardrail.main.sensitive_information_policy_config[0].pii_entities_config : p.type if p.action == "BLOCK"
    ]) == toset(["AWS_ACCESS_KEY", "AWS_SECRET_KEY", "PASSWORD", "PIN", "CREDIT_DEBIT_CARD_NUMBER", "CREDIT_DEBIT_CARD_CVV", "US_BANK_ACCOUNT_NUMBER", "INTERNATIONAL_BANK_ACCOUNT_NUMBER"])
    error_message = "credentials and payment secrets are blocked outright"
  }
  assert {
    condition     = length(aws_bedrock_guardrail.main.topic_policy_config[0].topics_config) > 0
    error_message = "at least one denied topic"
  }
}

run "the_core_uses_its_browser_and_guardrail_only" {
  command = apply

  assert {
    condition = jsondecode(aws_iam_role_policy.core_tools.policy).Statement == [
      {
        Effect   = "Allow"
        Action   = ["bedrock-agentcore:StartBrowserSession", "bedrock-agentcore:StopBrowserSession", "bedrock-agentcore:ConnectBrowserAutomationStream"]
        Resource = aws_bedrockagentcore_browser.main.browser_arn
      },
      {
        Effect   = "Allow"
        Action   = "bedrock:ApplyGuardrail"
        Resource = aws_bedrock_guardrail.main.guardrail_arn
      },
    ]
    error_message = "one browser, one guardrail; no live view until sessions outlive a call"
  }
  assert {
    condition = alltrue([for k in ["MANDATE_BROWSER_ID", "MANDATE_GUARDRAIL_ID", "MANDATE_GUARDRAIL_VERSION"] :
    contains([for e in jsondecode(aws_ecs_task_definition.core.container_definitions)[0].environment : e.name], k)])
    error_message = "the core's task definition carries the browser and guardrail ids"
  }
  assert {
    condition     = jsondecode(aws_iam_role_policy.browser_recordings.policy).Statement[0].Resource == "${aws_s3_bucket.recordings.arn}/recordings/*"
    error_message = "the browser may write recordings under its prefix only"
  }
}

run "only_the_dev_environment_of_this_repo_can_deploy" {
  command = apply

  assert {
    condition = jsondecode(aws_iam_role.deploy.assume_role_policy).Statement[0].Condition == {
      StringEquals = {
        "token.actions.githubusercontent.com:aud" = "sts.amazonaws.com"
        "token.actions.githubusercontent.com:sub" = "repo:Shubham04-oss/mandate:environment:dev"
      }
    }
    error_message = "GitHub OIDC for the dev environment of this repository only; no long-lived keys"
  }
  assert {
    condition = toset(flatten([
      for st in jsondecode(aws_iam_role_policy.deploy.policy).Statement : st.Action if st.Resource == "*"
    ])) == toset(["ecr:GetAuthorizationToken", "ecs:DescribeTaskDefinition", "ecs:RegisterTaskDefinition"])
    error_message = "only actions without resource-level permissions use a wildcard"
  }
  assert {
    condition = one([
      for st in jsondecode(aws_iam_role_policy.deploy.policy).Statement : st if contains(flatten([st.Action]), "iam:PassRole")
    ]).Condition == { StringEquals = { "iam:PassedToService" = "ecs-tasks.amazonaws.com" } }
    error_message = "the deploy role passes the task roles to ECS only"
  }
  assert {
    condition = toset(one([
      for st in jsondecode(aws_iam_role_policy.deploy.policy).Statement : st.Resource if contains(flatten([st.Action]), "iam:PassRole")
    ])) == toset(concat([for r in aws_iam_role.execution : r.arn], [aws_iam_role.core_task.arn]))
    error_message = "and only the task roles"
  }
}
