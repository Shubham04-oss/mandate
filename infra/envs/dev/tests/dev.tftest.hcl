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
# Mocks leave computed blocks empty; RDS always returns the managed secret when it manages the password.
override_resource {
  target = aws_db_instance.main
  values = {
    address            = "mandate-dev.mock.ap-south-1.rds.amazonaws.com"
    master_user_secret = [{ secret_arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:rds!db-mock", kms_key_id = "arn:aws:kms:ap-south-1:000000000000:key/mock", secret_status = "active" }]
  }
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
  target = aws_security_group.db_bootstrap
  values = { id = "sg-db-bootstrap" }
}
override_resource {
  target = aws_security_group.endpoints
  values = { id = "sg-endpoints" }
}
override_resource {
  target = aws_vpc_endpoint.s3
  values = { prefix_list_id = "pl-s3" }
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
  target = aws_iam_role.db_bootstrap_execution
  values = { arn = "arn:aws:iam::000000000000:role/mandate-dev-db-bootstrap-execution" }
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
    error_message = "two AZs: the RDS subnet group and the ALB both need two subnets"
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

run "database_is_private_encrypted_and_never_holds_a_password_in_state" {
  command = apply

  assert {
    condition     = !aws_db_instance.main.publicly_accessible && !aws_db_instance.main.multi_az
    error_message = "one private instance, never public"
  }
  assert {
    condition     = aws_db_instance.main.storage_encrypted && aws_db_instance.main.kms_key_id == aws_kms_key.main.arn
    error_message = "storage encrypted with the CMK"
  }
  assert {
    condition     = aws_db_instance.main.manage_master_user_password == true && aws_db_instance.main.password == null && aws_db_instance.main.master_user_secret_kms_key_id == aws_kms_key.main.arn
    error_message = "RDS manages the master password in Secrets Manager under the CMK; it never enters state"
  }
  assert {
    condition     = one([for p in aws_db_parameter_group.main.parameter : p.value if p.name == "rds.force_ssl"]) == "1" && aws_db_instance.main.parameter_group_name == aws_db_parameter_group.main.name
    error_message = "connections must use TLS"
  }
  assert {
    condition     = aws_db_instance.main.ca_cert_identifier == "rds-ca-rsa2048-g1"
    error_message = "the server certificate chains to the RSA2048 G1 root in rds-ca/"
  }
  assert {
    condition     = var.db_instance_class == "db.t4g.micro" && aws_db_instance.main.instance_class == var.db_instance_class
    error_message = "dev runs on db.t4g.micro unless the variable says otherwise"
  }
  assert {
    condition     = aws_db_instance.main.deletion_protection && !aws_db_instance.main.skip_final_snapshot && aws_db_instance.main.backup_retention_period == 7
    error_message = "protected from deletion, final snapshot kept, a week of backups"
  }
}

run "db_bootstrap_takes_role_passwords_from_secrets_only" {
  command = apply

  assert {
    condition     = jsondecode(aws_ecs_task_definition.db_bootstrap.container_definitions)[0].user == "postgres"
    error_message = "the shell holding the master password does not run as root"
  }
  assert {
    condition = toset([for s in jsondecode(aws_ecs_task_definition.db_bootstrap.container_definitions)[0].secrets : s.valueFrom]) == toset([
      "${one(aws_db_instance.main.master_user_secret[*].secret_arn)}:password::",
      aws_secretsmanager_secret.this["core/database-url"].arn,
      aws_secretsmanager_secret.this["eve/database-url"].arn,
    ])
    error_message = "the master password and the two role URLs arrive as task secrets"
  }
  assert {
    condition     = !anytrue([for e in jsondecode(aws_ecs_task_definition.db_bootstrap.container_definitions)[0].environment : e.name == "PGPASSWORD" || endswith(e.name, "_URL")])
    error_message = "no password is a plain environment value"
  }
  assert {
    condition     = one([for e in jsondecode(aws_ecs_task_definition.db_bootstrap.container_definitions)[0].environment : e.value if e.name == "PGSSLMODE"]) == "verify-full"
    error_message = "the bootstrap verifies the server certificate and host name"
  }
  assert {
    condition     = one([for e in jsondecode(aws_ecs_task_definition.db_bootstrap.container_definitions)[0].environment : e.value if e.name == "RDS_CA_PEM"]) == file("${path.module}/rds-ca/ap-south-1-bundle.crt")
    error_message = "the bootstrap trusts exactly the RDS CA bundle"
  }
  assert {
    condition     = startswith(jsondecode(aws_ecs_task_definition.db_bootstrap.container_definitions)[0].image, "${aws_ecr_repository.this["core"].repository_url}@") && can(regex("@sha256:[0-9a-f]{64}$", jsondecode(aws_ecs_task_definition.db_bootstrap.container_definitions)[0].image))
    error_message = "the image that holds the master password comes from the private repository, pinned by digest"
  }
  assert {
    condition     = !contains(flatten([for st in jsondecode(aws_iam_role_policy.deploy.policy).Statement : st.Resource if contains(flatten([st.Action]), "iam:PassRole")]), aws_iam_role.db_bootstrap_execution.arn)
    error_message = "the deploy workflow cannot run the bootstrap"
  }
}

run "migrate_reads_the_master_password_live" {
  command = apply

  assert {
    condition     = [for s in jsondecode(aws_ecs_task_definition.migrate.container_definitions)[0].secrets : s if s.name == "PGPASSWORD"] == [{ name = "PGPASSWORD", valueFrom = "${one(aws_db_instance.main.master_user_secret[*].secret_arn)}:password::" }]
    error_message = "migrate takes the password from the RDS-managed secret, so rotation never stales it"
  }
  assert {
    condition     = one([for e in jsondecode(aws_ecs_task_definition.migrate.container_definitions)[0].environment : e.value if e.name == "MANDATE_MIGRATION_DATABASE_URL"]) == "postgres://mandate_migrator@mandate-dev.mock.ap-south-1.rds.amazonaws.com:5432/mandate?sslmode=verify-full&sslrootcert=/run/mandate/rds-ca.pem"
    error_message = "the migration URL holds no password and verifies the server"
  }
}

run "every_database_client_gets_the_pinned_rds_ca" {
  command = apply

  assert {
    condition     = filesha256("${path.module}/rds-ca/ap-south-1-bundle.crt") == "ca4a9dc14e06c3f84274eff3ffed0e5d4d3463141593e1159eb4a0904df6cd74"
    error_message = "the RDS CA bundle is the one fetched from truststore.pki.rds.amazonaws.com; re-check before changing the hash"
  }
  assert {
    condition = alltrue([for td in [aws_ecs_task_definition.core, aws_ecs_task_definition.eve, aws_ecs_task_definition.migrate] :
    one([for e in jsondecode(td.container_definitions)[0].environment : e.value if e.name == "RDS_CA_PEM"]) == file("${path.module}/rds-ca/ap-south-1-bundle.crt")])
    error_message = "the core, eve and migrate get the CA their verify-full URLs name"
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
      "eve/database-url", "eve/ai-gateway-key", "eve/assertion-public-key",
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

run "only_this_zone_reaches_the_core_so_cf_connecting_ip_is_trusted" {
  command = apply

  assert {
    condition = one(flatten([
      for c in aws_lb_listener_rule.api.condition : [for h in c.http_header : h if h.http_header_name == "X-Mandate-Origin-Auth"]
    ])).values == toset([random_password.origin.result])
    error_message = "the ALB forwards only requests carrying the secret origin header"
  }
  assert {
    condition     = length(random_password.origin.result) == 48 && !random_password.origin.special
    error_message = "a 48-character alphanumeric origin secret (ALB header values allow no specials)"
  }
  assert {
    condition = (
      cloudflare_ruleset.origin_header.phase == "http_request_late_transform"
      && cloudflare_ruleset.origin_header.rules[0].expression == "(http.host eq \"api.example.test\")"
      && cloudflare_ruleset.origin_header.rules[0].action_parameters.headers["X-Mandate-Origin-Auth"].value == random_password.origin.result
    )
    error_message = "Cloudflare sets the origin header on the API host"
  }
  assert {
    condition = {
      for e in jsondecode(aws_ecs_task_definition.core.container_definitions)[0].environment : e.name => e.value
      if contains(["MANDATE_TRUST_CF_IP", "MANDATE_API_ORIGIN", "MANDATE_WEB_ORIGIN"], e.name)
      } == {
      MANDATE_TRUST_CF_IP = "1"
      MANDATE_API_ORIGIN  = "https://api.example.test"
      MANDATE_WEB_ORIGIN  = "https://app.example.test"
    }
    error_message = "the core trusts CF-Connecting-IP and has both origins, so its web API starts"
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
    ])) == toset([one(aws_db_instance.main.master_user_secret[*].secret_arn)])
    error_message = "the migrate task reads only the RDS-managed master secret"
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

run "the_one_off_tasks_reach_only_the_endpoints_and_the_database" {
  command = apply

  assert {
    condition     = !contains(keys(aws_vpc_security_group_egress_rule.https), "migrate") && !contains(keys(aws_vpc_security_group_egress_rule.https), "db-bootstrap")
    error_message = "migrate and db-bootstrap have no route to the internet"
  }
  assert {
    condition = alltrue([for t, sg in { migrate = aws_security_group.migrate.id, "db-bootstrap" = aws_security_group.db_bootstrap.id } : (
      aws_vpc_security_group_egress_rule.to_endpoints[t].security_group_id == sg
      && aws_vpc_security_group_egress_rule.to_endpoints[t].referenced_security_group_id == aws_security_group.endpoints.id
      && aws_vpc_security_group_egress_rule.to_s3[t].security_group_id == sg
      && aws_vpc_security_group_egress_rule.to_s3[t].prefix_list_id == aws_vpc_endpoint.s3.prefix_list_id
      && aws_vpc_security_group_egress_rule.to_db[t].referenced_security_group_id == aws_security_group.db.id
    )])
    error_message = "HTTPS to the interface and S3 endpoints, Postgres to the database"
  }
}

run "the_database_admits_only_the_tasks" {
  command = apply

  assert {
    condition = toset([for r in aws_vpc_security_group_ingress_rule.db : r.referenced_security_group_id]) == toset([
      aws_security_group.core.id, aws_security_group.eve.id, aws_security_group.migrate.id, aws_security_group.db_bootstrap.id,
    ])
    error_message = "Postgres from the core, eve, migrate and db-bootstrap only"
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
        "token.actions.githubusercontent.com:aud"              = "sts.amazonaws.com"
        "token.actions.githubusercontent.com:sub"              = "repo:Shubham04-oss/mandate:environment:dev"
        "token.actions.githubusercontent.com:job_workflow_ref" = "Shubham04-oss/mandate/.github/workflows/deploy.yml@refs/heads/main"
      }
    }
    error_message = "GitHub OIDC for main's deploy workflow in the dev environment of this repository only"
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

run "an_existing_github_oidc_provider_is_reused_not_owned" {
  command = plan
  variables {
    github_oidc_provider_arn = "arn:aws:iam::000000000000:oidc-provider/token.actions.githubusercontent.com"
  }
  assert {
    condition     = length(aws_iam_openid_connect_provider.github) == 0
    error_message = "with an existing provider ARN, this stack must not create or own one"
  }
  assert {
    condition     = jsondecode(aws_iam_role.deploy.assume_role_policy).Statement[0].Principal.Federated == "arn:aws:iam::000000000000:oidc-provider/token.actions.githubusercontent.com"
    error_message = "the deploy role must trust the given provider"
  }
}
