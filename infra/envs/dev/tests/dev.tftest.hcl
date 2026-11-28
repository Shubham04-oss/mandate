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
  mock_resource "aws_service_discovery_service" {
    defaults = { arn = "arn:aws:servicediscovery:ap-south-1:000000000000:service/srv-mock" }
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
  mock_resource "aws_sns_topic" {
    defaults = { arn = "arn:aws:sns:ap-south-1:000000000000:mandate-dev-alarms" }
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
  mock_resource "cloudflare_zero_trust_tunnel_cloudflared" {
    defaults = { id = "00000000-0000-4000-8000-000000000001" }
  }
  mock_data "cloudflare_zero_trust_tunnel_cloudflared_token" {
    defaults = { token = "mock-tunnel-token" }
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
override_resource {
  target = aws_secretsmanager_secret.this["posthog-key"]
  values = { arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/posthog-key" }
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
  target = aws_subnet.public[0]
  values = { id = "subnet-public-a" }
}
override_resource {
  target = aws_subnet.public[1]
  values = { id = "subnet-public-b" }
}
override_resource {
  target = aws_subnet.private[0]
  values = { id = "subnet-private-a" }
}
override_resource {
  target = aws_subnet.private[1]
  values = { id = "subnet-private-b" }
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
  target = aws_security_group.collector[0]
  values = { id = "sg-collector" }
}
override_resource {
  target = aws_secretsmanager_secret.tunnel_token
  # A secret's id is its ARN.
  values = {
    arn = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/cloudflared/tunnel-token"
    id  = "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/cloudflared/tunnel-token"
  }
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
  target = aws_sqs_queue.consumer["posthog"]
  values = { arn = "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-posthog.fifo", url = "https://sqs.ap-south-1.amazonaws.com/000000000000/mandate-dev-posthog.fifo" }
}
override_resource {
  target = aws_sqs_queue.dead_letter["posthog"]
  values = { arn = "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-posthog-dlq.fifo" }
}
override_resource {
  target = aws_sqs_queue.consumer["email"]
  values = { arn = "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-email.fifo", url = "https://sqs.ap-south-1.amazonaws.com/000000000000/mandate-dev-email.fifo" }
}
override_resource {
  target = aws_sqs_queue.dead_letter["email"]
  values = { arn = "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-email-dlq.fifo" }
}
override_resource {
  target = aws_sqs_queue.email_feedback[0]
  # A queue's id is its URL.
  values = {
    arn = "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-email-feedback"
    url = "https://sqs.ap-south-1.amazonaws.com/000000000000/mandate-dev-email-feedback"
    id  = "https://sqs.ap-south-1.amazonaws.com/000000000000/mandate-dev-email-feedback"
  }
}
override_resource {
  target = aws_sqs_queue.email_feedback_dead_letter[0]
  values = { arn = "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-email-feedback-dlq" }
}
override_resource {
  target = aws_sesv2_configuration_set.main[0]
  values = { arn = "arn:aws:ses:ap-south-1:000000000000:configuration-set/mandate-dev" }
}
override_resource {
  target = aws_cloudwatch_event_rule.email_feedback[0]
  values = { arn = "arn:aws:events:ap-south-1:000000000000:rule/mandate-dev-email-feedback" }
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
  domain                = "example.test"
  cloudflare_zone_id    = "0123456789abcdef0123456789abcdef"
  cloudflare_account_id = "fedcba9876543210fedcba9876543210"
  ai_gateway_url        = "https://gateway.example.test"
  model_default         = "model"
}

run "nothing_gives_the_database_a_public_route" {
  command = apply

  assert {
    condition     = alltrue([for s in aws_subnet.private : !s.map_public_ip_on_launch])
    error_message = "private subnets must not hand out public IPs"
  }
  assert {
    condition     = length(aws_subnet.private) == 2 && length(aws_subnet.public) == 2
    error_message = "two AZs: the RDS subnet group needs two subnets"
  }
  assert {
    condition     = length(aws_route_table.private.route) == 0
    error_message = "the private route table has no route out of the VPC (no NAT, no internet gateway)"
  }
  assert {
    condition = (
      toset(aws_db_subnet_group.main.subnet_ids) == toset(aws_subnet.private[*].id)
      && toset([for a in aws_route_table_association.private : a.subnet_id]) == toset(aws_subnet.private[*].id)
      && alltrue([for a in aws_route_table_association.private : a.route_table_id == aws_route_table.private.id])
      && length(setintersection(toset([for a in aws_route_table_association.public : a.subnet_id]), toset(aws_subnet.private[*].id))) == 0
    )
    error_message = "the database lives only in the private subnets, which use only the private route table"
  }
  assert {
    condition     = !aws_db_instance.main.publicly_accessible
    error_message = "the database has no public address"
  }
}

run "default_security_group_allows_nothing" {
  command = apply

  assert {
    condition     = length(aws_default_security_group.locked.ingress) == 0 && length(aws_default_security_group.locked.egress) == 0
    error_message = "the default security group must have no rules"
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
      "eve/database-url", "eve/ai-gateway-key", "eve/assertion-public-key", "posthog-key",
    ])
    error_message = "one secret per value a task needs, named by who reads it (posthog-key: the collector, the core and eve)"
  }
  assert {
    condition     = aws_kms_key.main.enable_key_rotation
    error_message = "the CMK rotates"
  }
}

run "the_tunnel_forwards_the_web_api_only" {
  command = apply

  assert {
    condition = jsonencode([for r in cloudflare_zero_trust_tunnel_cloudflared_config.api.config.ingress : [r.hostname, r.path, r.service]]) == jsonencode([
      ["api.example.test", "^/(v1/.*|healthz)$", "http://localhost:8080"],
      [null, null, "http_status:404"],
    ])
    error_message = "api.<domain> /v1/* and /healthz go to the core on localhost:8080; anything else is a 404 (the MCP server stays private)"
  }
  assert {
    condition     = cloudflare_zero_trust_tunnel_cloudflared.api.config_src == "cloudflare" && cloudflare_zero_trust_tunnel_cloudflared_config.api.tunnel_id == cloudflare_zero_trust_tunnel_cloudflared.api.id
    error_message = "a remotely managed tunnel, configured here"
  }
}

run "the_core_admits_only_eve_so_cf_connecting_ip_comes_from_the_sidecar" {
  command = apply

  assert {
    condition = alltrue([
      for r in concat([aws_vpc_security_group_ingress_rule.core_from_eve, aws_vpc_security_group_ingress_rule.eve_from_core], values(aws_vpc_security_group_ingress_rule.db)) :
      r.referenced_security_group_id == aws_security_group.eve.id && r.cidr_ipv4 == null && r.cidr_ipv6 == null && r.prefix_list_id == null
      if r.security_group_id == aws_security_group.core.id
      ]) && length([
      for r in concat([aws_vpc_security_group_ingress_rule.core_from_eve, aws_vpc_security_group_ingress_rule.eve_from_core], values(aws_vpc_security_group_ingress_rule.db)) : r if r.security_group_id == aws_security_group.core.id
    ]) == 1
    error_message = "the core's only ingress rule is from eve, by security group; no address or prefix list"
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

run "cloudflared_runs_beside_the_core_pinned_and_unprivileged" {
  command = apply

  assert {
    condition     = [for c in jsondecode(aws_ecs_task_definition.core.container_definitions) : c.name] == ["core", "cloudflared"]
    error_message = "the core stays first: the deploy workflow swaps containerDefinitions[0]'s image"
  }
  assert {
    condition     = can(regex("^docker\\.io/cloudflare/cloudflared@sha256:[0-9a-f]{64}$", jsondecode(aws_ecs_task_definition.core.container_definitions)[1].image))
    error_message = "the official cloudflared image, pinned by digest"
  }
  assert {
    condition = (
      jsondecode(aws_ecs_task_definition.core.container_definitions)[1].user == "65532:65532"
      && jsondecode(aws_ecs_task_definition.core.container_definitions)[1].command == ["tunnel", "--no-autoupdate", "run"]
      && jsondecode(aws_ecs_task_definition.core.container_definitions)[1].logConfiguration.options["awslogs-stream-prefix"] == "cloudflared"
    )
    error_message = "cloudflared runs non-root, never self-updates, and logs to its own stream"
  }
  assert {
    condition     = jsondecode(aws_ecs_task_definition.core.container_definitions)[1].secrets == [{ name = "TUNNEL_TOKEN", valueFrom = aws_secretsmanager_secret.tunnel_token.arn }]
    error_message = "the tunnel token arrives as a task secret"
  }
  assert {
    condition     = !contains([for s in jsondecode(aws_ecs_task_definition.core.container_definitions)[0].secrets : s.valueFrom], aws_secretsmanager_secret.tunnel_token.arn)
    error_message = "the core container never sees the tunnel token"
  }
  assert {
    condition     = aws_secretsmanager_secret.tunnel_token.kms_key_id == aws_kms_key.main.arn && aws_secretsmanager_secret_version.tunnel_token.secret_string == "mock-tunnel-token"
    error_message = "the tunnel's own token, under the CMK"
  }
  assert {
    condition     = aws_secretsmanager_secret_version.tunnel_token.secret_id == "arn:aws:secretsmanager:ap-south-1:000000000000:secret:mandate-dev/cloudflared/tunnel-token"
    error_message = "the token is written into the tunnel secret, not another one"
  }
  assert {
    condition = jsondecode(aws_ecs_task_definition.core.container_definitions)[0].healthCheck == {
      command     = ["CMD", "/usr/local/bin/mandate", "healthcheck"]
      interval    = 10
      timeout     = 5
      retries     = 3
      startPeriod = 30
    }
    error_message = "the core has a health check: its own binary probing /healthz"
  }
  assert {
    condition     = jsondecode(aws_ecs_task_definition.core.container_definitions)[1].dependsOn == [{ containerName = "core", condition = "HEALTHY" }]
    error_message = "cloudflared starts only once the core is healthy"
  }
  assert {
    condition = contains(flatten([
      for st in jsondecode(aws_iam_role_policy.execution["core"].policy).Statement : st.Resource if contains(flatten([st.Action]), "secretsmanager:GetSecretValue")
    ]), aws_secretsmanager_secret.tunnel_token.arn)
    error_message = "the core's execution role can inject the tunnel token"
  }
  assert {
    condition     = toset([for r in aws_vpc_security_group_egress_rule.core_to_tunnel : "${r.ip_protocol}/${r.from_port}-${r.to_port}"]) == toset(["tcp/7844-7844", "udp/7844-7844"])
    error_message = "cloudflared reaches Cloudflare's edge on 7844 only"
  }
}

run "eve_is_private_and_reachable_only_from_the_core" {
  command = apply

  assert {
    condition = alltrue([
      for r in concat([aws_vpc_security_group_ingress_rule.core_from_eve, aws_vpc_security_group_ingress_rule.eve_from_core], values(aws_vpc_security_group_ingress_rule.db)) :
      r.referenced_security_group_id == aws_security_group.core.id && r.cidr_ipv4 == null && r.cidr_ipv6 == null && r.prefix_list_id == null
      if r.security_group_id == aws_security_group.eve.id
      ]) && length([
      for r in concat([aws_vpc_security_group_ingress_rule.core_from_eve, aws_vpc_security_group_ingress_rule.eve_from_core], values(aws_vpc_security_group_ingress_rule.db)) : r if r.security_group_id == aws_security_group.eve.id
    ]) == 1
    error_message = "eve's only ingress rule is from the core, by security group; no address or prefix list"
  }
  assert {
    condition     = length(aws_ecs_service.eve.load_balancer) == 0
    error_message = "eve has no load balancer"
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

run "services_run_on_spot_in_public_subnets_without_nat" {
  command = apply

  assert {
    condition = alltrue([for svc in [aws_ecs_service.core, aws_ecs_service.eve] : (
      length(svc.capacity_provider_strategy) == 1 && one(svc.capacity_provider_strategy).capacity_provider == "FARGATE_SPOT" && one(svc.capacity_provider_strategy).weight == 1
    )])
    error_message = "core and eve run on Fargate Spot when use_fargate_spot is true (the default)"
  }
  assert {
    condition     = toset(aws_ecs_cluster_capacity_providers.main.capacity_providers) == toset(["FARGATE", "FARGATE_SPOT"])
    error_message = "the cluster offers Spot and on-demand Fargate"
  }
  assert {
    condition = alltrue([for svc in [aws_ecs_service.core, aws_ecs_service.eve] : (
      svc.network_configuration[0].assign_public_ip && toset(svc.network_configuration[0].subnets) == toset(aws_subnet.public[*].id)
    )])
    error_message = "the services egress through their own public IPs, not a NAT gateway"
  }
  assert {
    condition     = alltrue([for o in [output.migrate_network_configuration, output.db_bootstrap_network_configuration] : jsondecode(o).awsvpcConfiguration.assignPublicIp == "ENABLED" && toset(jsondecode(o).awsvpcConfiguration.subnets) == toset(aws_subnet.public[*].id)])
    error_message = "the one-off tasks run in the public subnets with a public IP"
  }
}

run "eve_stops_before_its_replacement_starts" {
  command = plan
  assert {
    condition     = aws_ecs_service.eve.deployment_minimum_healthy_percent == 0 && aws_ecs_service.eve.deployment_maximum_percent == 100
    error_message = "eve's Postgres world serves one eve version at a time: a deploy never overlaps two eve tasks"
  }
}

run "spot_can_be_switched_off" {
  command = plan
  variables {
    use_fargate_spot = false
  }
  assert {
    condition     = alltrue([for svc in [aws_ecs_service.core, aws_ecs_service.eve] : one(svc.capacity_provider_strategy).capacity_provider == "FARGATE"])
    error_message = "use_fargate_spot = false puts the services on on-demand Fargate"
  }
}

run "images_are_immutable" {
  command = apply

  assert {
    condition     = alltrue([for r in aws_ecr_repository.this : r.image_tag_mutability == "IMMUTABLE"])
    error_message = "a deployed tag always names the same image"
  }
}

run "api_is_proxied_and_auth_is_rate_limited_at_the_edge" {
  command = apply

  assert {
    condition     = cloudflare_dns_record.api.proxied && cloudflare_dns_record.api.type == "CNAME" && cloudflare_dns_record.api.content == "00000000-0000-4000-8000-000000000001.cfargotunnel.com"
    error_message = "api.<domain> is a proxied CNAME to the tunnel"
  }
  assert {
    condition     = cloudflare_ruleset.auth_rate_limit.phase == "http_ratelimit" && strcontains(cloudflare_ruleset.auth_rate_limit.rules[0].expression, "/v1/auth/")
    error_message = "WAF rate limit on /v1/auth/*"
  }
}

run "the_one_off_tasks_take_nothing_in_and_reach_https_and_the_database" {
  command = apply

  assert {
    condition = alltrue([for t, sg in { migrate = aws_security_group.migrate, "db-bootstrap" = aws_security_group.db_bootstrap } : (
      length(sg.ingress) == 0
      && aws_vpc_security_group_egress_rule.https[t].security_group_id == sg.id
      && aws_vpc_security_group_egress_rule.https[t].from_port == 443 && aws_vpc_security_group_egress_rule.https[t].to_port == 443
      && aws_vpc_security_group_egress_rule.to_db[t].referenced_security_group_id == aws_security_group.db.id
    )])
    error_message = "no ingress; HTTPS out (dev trade-off: no endpoints) and Postgres to the database"
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
    condition     = keys(aws_sqs_queue.consumer) == ["agent-resume", "posthog"] && alltrue([for q in aws_sqs_queue.consumer : q.fifo_queue && q.sqs_managed_sse_enabled])
    error_message = "one encrypted FIFO queue per consumer: agent-resume and posthog"
  }
  assert {
    condition = toset(jsondecode(awscc_eventsv2_subscriber.consumer["posthog"].filter_configuration.filters[0].pattern)["aws:DetailType"]) == toset([
      "approval.requested.v1", "approval.decided.v1", "conversation.started.v1", "conversation.finished.v1",
      "grant.created.v1", "grant.revoked.v1", "member.created.v1", "member.updated.v1", "member.removed.v1",
      "organization.created.v1", "member.signed_up.v1", "member.signed_in.v1", "organization.chosen.v1",
      "setting.changed.v1", "email.sent.v1", "email.bounced.v1", "email.complained.v1",
    ]) && awscc_eventsv2_subscriber.consumer["posthog"].filter_configuration.filters[0].scope == "SYSTEM_METADATA"
    error_message = "posthog receives every core event type, matched on the trusted detail type"
  }
  assert {
    condition = {
      for e in jsondecode(aws_ecs_task_definition.core.container_definitions)[0].environment : e.name => e.value if startswith(e.name, "MANDATE_QUEUE_")
      } == {
      MANDATE_QUEUE_AGENT_RESUME = aws_sqs_queue.consumer["agent-resume"].url
      MANDATE_QUEUE_POSTHOG      = "https://sqs.ap-south-1.amazonaws.com/000000000000/mandate-dev-posthog.fifo"
    }
    error_message = "the core gets one MANDATE_QUEUE_<NAME> per consumer, posthog included even with telemetry off"
  }
  assert {
    condition     = keys(aws_cloudwatch_metric_alarm.dlq) == ["agent-resume", "posthog"]
    error_message = "the posthog dead-letter queue has an alarm too"
  }
  assert {
    condition     = alltrue([for k, q in aws_sqs_queue.consumer : jsondecode(q.redrive_policy).deadLetterTargetArn == aws_sqs_queue.dead_letter[k].arn])
    error_message = "every consumer queue redrives to its own dead-letter queue"
  }
  assert {
    # The core publishes with EventGroupId = tenant and DeduplicationId = event id (publisher.rs).
    condition = alltrue([for s in awscc_eventsv2_subscriber.consumer : s.type == "FIFO" &&
      s.invoke_configuration.sqs_parameters.message_group_id == "{% $events.SystemMetadata.EventGroupId %}" &&
    s.invoke_configuration.sqs_parameters.message_deduplication_id == "{% $events.SystemMetadata.DeduplicationId %}"])
    error_message = "FIFO delivery grouped by the publisher's EventGroupId (the tenant) and deduplicated by its id"
  }
  assert {
    # For PutEvents the DATA scope is EventBridge's envelope (detail is nested), so match the
    # trusted aws:DetailType system field instead.
    condition     = awscc_eventsv2_subscriber.consumer["agent-resume"].filter_configuration.filters[0].scope == "SYSTEM_METADATA" && jsondecode(awscc_eventsv2_subscriber.consumer["agent-resume"].filter_configuration.filters[0].pattern) == { "aws:DetailType" = ["approval.decided.v1"] }
    error_message = "agent-resume receives approval.decided.v1 only, matched on the trusted detail type"
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
        Resource = [aws_sqs_queue.consumer["agent-resume"].arn, "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-posthog.fifo"]
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
        "token.actions.githubusercontent.com:sub"              = "repo:Shubham04-oss@218535561/mandate@1466076280:environment:dev"
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

run "alarms_cover_every_dlq_and_service_with_no_default_subscriber" {
  command = apply
  assert {
    condition     = keys(aws_cloudwatch_metric_alarm.dlq) == keys(aws_sqs_queue.dead_letter)
    error_message = "every dead-letter queue has an alarm"
  }
  assert {
    condition     = alltrue([for a in aws_cloudwatch_metric_alarm.service_down : a.treat_missing_data == "breaching"])
    error_message = "a service that reports nothing is down"
  }
  assert {
    condition     = keys(aws_cloudwatch_metric_alarm.consumer_stuck) == ["agent-resume"]
    error_message = "every consumer queue that is read has a stuck alarm; the unread posthog queue has none while telemetry is off"
  }
  assert {
    condition     = alltrue([for a in aws_cloudwatch_metric_alarm.consumer_stuck : a.metric_name == "ApproximateAgeOfOldestMessage" && a.threshold == 900])
    error_message = "a stuck consumer with the default 5 receives is one whose oldest message is over 15 minutes old"
  }
  assert {
    condition     = length(aws_sns_topic_subscription.alarm_email) == 0
    error_message = "no subscription is created unless alarm_email is set"
  }
  assert {
    condition     = aws_cloudwatch_metric_alarm.db_free_storage.threshold == 20 * 0.15 * 1024 * 1024 * 1024
    error_message = "the storage alarm fires below 15% of the allocated 20 GB"
  }
  assert {
    condition     = strcontains(jsondecode(aws_ecs_task_definition.eve.container_definitions)[0].healthCheck.command[3], "http://127.0.0.1:3000/eve/v1/health")
    error_message = "eve's health check probes its own health route"
  }
}

run "telemetry_is_off_by_default_and_changes_nothing" {
  command = apply

  assert {
    condition = (
      length(aws_ecs_service.collector) == 0 && length(aws_ecs_task_definition.collector) == 0
      && length(aws_security_group.collector) == 0 && length(aws_iam_role.collector_execution) == 0
      && length(aws_vpc_security_group_ingress_rule.collector) == 0 && length(aws_vpc_security_group_egress_rule.to_collector) == 0
      && !contains(keys(aws_service_discovery_service.this), "collector") && !contains(keys(aws_cloudwatch_log_group.this), "collector")
    )
    error_message = "with telemetry_enabled false there is no collector at all"
  }
  assert {
    condition = !anytrue([for td in [aws_ecs_task_definition.core, aws_ecs_task_definition.eve] : anytrue([
      for e in jsondecode(td.container_definitions)[0].environment : startswith(e.name, "OTEL_") || startswith(e.name, "POSTHOG_")
    ])])
    error_message = "no OTEL or PostHog env on the core or eve while telemetry is off"
  }
  assert {
    condition = !anytrue([for td in [aws_ecs_task_definition.core, aws_ecs_task_definition.eve] : contains(
      [for s in jsondecode(td.container_definitions)[0].secrets : s.valueFrom], aws_secretsmanager_secret.this["posthog-key"].arn
    )])
    error_message = "the PostHog key reaches no task while telemetry is off"
  }
  assert {
    condition = !anytrue([for t in ["core", "eve", "migrate"] : contains(flatten([
      for st in jsondecode(aws_iam_role_policy.execution[t].policy).Statement : st.Resource if contains(flatten([st.Action]), "secretsmanager:GetSecretValue")
    ]), aws_secretsmanager_secret.this["posthog-key"].arn)])
    error_message = "no execution role can read the PostHog key while telemetry is off"
  }
  assert {
    condition = !anytrue([for td in [aws_ecs_task_definition.core, aws_ecs_task_definition.eve] : anytrue([
      for e in jsondecode(td.container_definitions)[0].environment : e.name == "MANDATE_DEBUG_ROUTES"
    ])])
    error_message = "the forced-error routes are off unless debug_routes is set"
  }
}

run "debug_routes_switches_on_the_forced_error_route_for_the_core_and_eve" {
  command = apply
  variables {
    debug_routes = true
  }

  assert {
    condition = alltrue([for td in [aws_ecs_task_definition.core, aws_ecs_task_definition.eve] : contains(
      jsondecode(td.container_definitions)[0].environment, { name = "MANDATE_DEBUG_ROUTES", value = "1" }
    )])
    error_message = "debug_routes sets MANDATE_DEBUG_ROUTES=1 on the core and eve"
  }
  assert {
    condition     = !contains(jsondecode(aws_ecs_task_definition.migrate.container_definitions)[0].environment, { name = "MANDATE_DEBUG_ROUTES", value = "1" })
    error_message = "the migrate task never mounts routes"
  }
}

run "the_collector_is_the_only_way_out_for_telemetry" {
  command = apply
  variables {
    telemetry_enabled = true
  }

  assert {
    condition     = keys(aws_cloudwatch_metric_alarm.consumer_stuck) == ["agent-resume", "posthog"]
    error_message = "with telemetry on, the posthog queue is read, so it gets a stuck alarm too"
  }
  assert {
    condition     = aws_cloudwatch_metric_alarm.consumer_stuck["posthog"].threshold == 50 * 60 + 300
    error_message = "the posthog alarm waits out its 50 retries (50 minutes) plus 5 before firing"
  }

  assert {
    condition = (
      toset([for r in aws_vpc_security_group_ingress_rule.collector : r.referenced_security_group_id]) == toset([aws_security_group.core.id, aws_security_group.eve.id])
      && alltrue([for r in aws_vpc_security_group_ingress_rule.collector : r.security_group_id == "sg-collector" && r.ip_protocol == "tcp" && r.from_port == 4318 && r.to_port == 4318 && r.cidr_ipv4 == null && r.cidr_ipv6 == null && r.prefix_list_id == null])
    )
    error_message = "the collector admits OTLP/HTTP on 4318 from the core and eve only, by security group"
  }
  assert {
    condition = length([
      for r in concat([aws_vpc_security_group_ingress_rule.core_from_eve, aws_vpc_security_group_ingress_rule.eve_from_core], values(aws_vpc_security_group_ingress_rule.db)) : r if r.security_group_id == "sg-collector"
    ]) == 0 && length(aws_security_group.collector[0].ingress) == 0
    error_message = "no other rule admits anything to the collector"
  }
  assert {
    condition = (
      aws_vpc_security_group_egress_rule.collector_https[0].security_group_id == "sg-collector"
      && aws_vpc_security_group_egress_rule.collector_https[0].cidr_ipv4 == "0.0.0.0/0"
      && aws_vpc_security_group_egress_rule.collector_https[0].from_port == 443 && aws_vpc_security_group_egress_rule.collector_https[0].to_port == 443
      && !contains([for r in aws_vpc_security_group_egress_rule.to_db : r.security_group_id], "sg-collector")
    )
    error_message = "the collector reaches HTTPS (PostHog) and not the database"
  }
  assert {
    condition = alltrue([for k, sg in { core = aws_security_group.core.id, eve = aws_security_group.eve.id } : (
      aws_vpc_security_group_egress_rule.to_collector[k].security_group_id == sg
      && aws_vpc_security_group_egress_rule.to_collector[k].referenced_security_group_id == "sg-collector"
      && aws_vpc_security_group_egress_rule.to_collector[k].from_port == 4318 && aws_vpc_security_group_egress_rule.to_collector[k].to_port == 4318
    )])
    error_message = "the core and eve reach the collector on 4318"
  }
  assert {
    condition = (
      aws_ecs_service.collector[0].desired_count == 1
      && aws_ecs_service.collector[0].network_configuration[0].assign_public_ip
      && toset(aws_ecs_service.collector[0].network_configuration[0].subnets) == toset(aws_subnet.public[*].id)
      && aws_ecs_service.collector[0].network_configuration[0].security_groups == toset(["sg-collector"])
      && one(aws_ecs_service.collector[0].service_registries).registry_arn == aws_service_discovery_service.this["collector"].arn
      && aws_service_discovery_service.this["collector"].name == "collector"
      && aws_ecs_task_definition.collector[0].cpu == "256" && aws_ecs_task_definition.collector[0].memory == "512"
    )
    error_message = "one small collector task in the public subnets, registered as collector.<ns>"
  }
  assert {
    condition = (
      can(regex("^docker\\.io/otel/opentelemetry-collector-contrib@sha256:[0-9a-f]{64}$", jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].image))
      && jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].user == "10001:10001"
      && jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].command == ["--config=env:OTELCOL_CONFIG"]
      && jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].portMappings == [{ containerPort = 4318, protocol = "tcp" }]
    )
    error_message = "the contrib collector, pinned by digest, non-root, config from the env, OTLP/HTTP port only"
  }
  assert {
    condition     = jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].secrets == [{ name = "POSTHOG_API_KEY", valueFrom = aws_secretsmanager_secret.this["posthog-key"].arn }]
    error_message = "the collector gets the PostHog key as a task secret, and nothing else"
  }
  assert {
    condition = toset(flatten([
      for st in jsondecode(aws_iam_role_policy.collector_execution[0].policy).Statement : st.Resource if contains(flatten([st.Action]), "secretsmanager:GetSecretValue")
    ])) == toset([aws_secretsmanager_secret.this["posthog-key"].arn])
    error_message = "the collector's execution role reads only the PostHog key"
  }
  assert {
    condition     = !contains(one([for st in jsondecode(aws_iam_role_policy.deploy.policy).Statement : st.Resource if contains(flatten([st.Action]), "iam:PassRole")]), aws_iam_role.collector_execution[0].arn)
    error_message = "the deploy workflow cannot run anything as the collector"
  }
  assert {
    condition = (
      contains(jsondecode(aws_ecs_task_definition.core.container_definitions)[0].secrets, { name = "POSTHOG_API_KEY", valueFrom = aws_secretsmanager_secret.this["posthog-key"].arn })
      && contains(flatten([
        for st in jsondecode(aws_iam_role_policy.execution["core"].policy).Statement : st.Resource if contains(flatten([st.Action]), "secretsmanager:GetSecretValue")
      ]), aws_secretsmanager_secret.this["posthog-key"].arn)
    )
    error_message = "the core gets the PostHog key for product events"
  }
  assert {
    condition = (
      contains(jsondecode(aws_ecs_task_definition.eve.container_definitions)[0].secrets, { name = "POSTHOG_API_KEY", valueFrom = aws_secretsmanager_secret.this["posthog-key"].arn })
      && contains(flatten([
        for st in jsondecode(aws_iam_role_policy.execution["eve"].policy).Statement : st.Resource if contains(flatten([st.Action]), "secretsmanager:GetSecretValue")
      ]), aws_secretsmanager_secret.this["posthog-key"].arn)
    )
    error_message = "eve gets the PostHog key for its errors and model calls (ORY-82)"
  }
  assert {
    condition = !contains(flatten([
      for st in jsondecode(aws_iam_role_policy.execution["migrate"].policy).Statement : st.Resource if contains(flatten([st.Action]), "secretsmanager:GetSecretValue")
    ]), aws_secretsmanager_secret.this["posthog-key"].arn)
    error_message = "the migrate task never sees the PostHog key"
  }
  assert {
    condition = {
      for e in jsondecode(aws_ecs_task_definition.core.container_definitions)[0].environment : e.name => e.value if startswith(e.name, "OTEL_") || startswith(e.name, "POSTHOG_")
      } == {
      OTEL_EXPORTER_OTLP_ENDPOINT = "http://collector.mandate-dev.internal:4318"
      OTEL_EXPORTER_OTLP_PROTOCOL = "http/protobuf"
      OTEL_SERVICE_NAME           = "mandate-core"
      POSTHOG_HOST                = "https://eu.i.posthog.com"
    }
    error_message = "the core exports to the collector as mandate-core and sends product events to PostHog EU"
  }
  assert {
    condition = {
      for e in jsondecode(aws_ecs_task_definition.eve.container_definitions)[0].environment : e.name => e.value if startswith(e.name, "OTEL_") || startswith(e.name, "POSTHOG_")
      } == {
      OTEL_EXPORTER_OTLP_ENDPOINT = "http://collector.mandate-dev.internal:4318"
      OTEL_EXPORTER_OTLP_PROTOCOL = "http/protobuf"
      OTEL_SERVICE_NAME           = "eve"
      POSTHOG_HOST                = "https://eu.i.posthog.com"
    }
    error_message = "eve exports to the collector as eve and sends its errors and model calls to PostHog EU"
  }
}

run "the_collector_config_lets_only_allowlisted_data_out" {
  command = apply
  variables {
    telemetry_enabled = true
  }

  assert {
    condition     = keys(yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).receivers) == ["otlp"]
    error_message = "one receiver: OTLP"
  }
  assert {
    condition     = yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).receivers.otlp.protocols == { http = { endpoint = "0.0.0.0:4318" } }
    error_message = "OTLP over HTTP on 4318 only, no gRPC"
  }
  assert {
    condition = yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).service.pipelines == {
      traces = { receivers = ["otlp"], processors = ["memory_limiter", "redaction", "transform", "batch"], exporters = ["otlp_http/posthog"] }
      logs   = { receivers = ["otlp"], processors = ["memory_limiter", "filter/warn", "redaction", "batch"], exporters = ["otlp_http/posthog"] }
    }
    error_message = "every pipeline redacts before it exports; logs below WARN are dropped; no sampling at 100%"
  }
  assert {
    condition     = yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors["filter/warn"].log_conditions == ["log.severity_number < SEVERITY_NUMBER_WARN"]
    error_message = "the logs pipeline keeps WARN and above only"
  }
  assert {
    condition = (
      yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.redaction.allow_all_keys == false
      && yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.redaction.ignored_keys == ["trace_id", "span_id"]
      && length(setintersection(toset(yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.redaction.allowed_keys), toset([
        "exception.message", "exception.stacktrace", "otel.status_description", "url.full", "url.path", "url.query",
        "db.statement", "db.query.text", "http.request.header.authorization", "user.email", "enduser.id", "client.address",
        "gen_ai.prompt", "gen_ai.completion",
      ]))) == 0
      && length(setsubtract(toset(["tenant.id", "agent.id", "conversation.id", "http.route", "http.request.method", "http.response.status_code", "otel.status_code", "error.type", "exception.type", "service.name"]),
      toset(yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.redaction.allowed_keys))) == 0
    )
    error_message = "an allowlist: ids, routes, codes and types pass; messages, stack traces, URLs, queries and user fields do not"
  }
  assert {
    # Fake look-alikes of each kind of secret; tofu's regex() is RE2 like the collector's Go regexp.
    condition = alltrue([for v in [
      "mail alice@example.com now", "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.c2lnbmF0dXJl", "Bearer abc.def",
      "sk-xxxxxxxxxxxxxxxxxxxxxxxx", "sk_test_xxxxxxxxxxxxxxxxxxxx", "phc_xxxxxxxxxxxxxxxxxxxxxxxx", "AKIAIOSFODNN7EXAMPLE",
      "xoxb-xxxxxxxxxxxx", "-----BEGIN PRIVATE KEY-----", "0123456789abcdef0123456789abcdef0123456789abcdef",
      "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIzNDU2Nzg5", "your code is 123456", "from 203.0.113.7",
      ] : anytrue([for re in yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.redaction.blocked_values : can(regex(re, v))])
    ])
    error_message = "emails, tokens, keys, long hex/base64, 6-digit codes and IPs are masked"
  }
  assert {
    condition = alltrue([for re in yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.redaction.blocked_values : anytrue([for v in [
      "mail alice@example.com now", "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.c2lnbmF0dXJl", "Bearer abc.def",
      "sk-xxxxxxxxxxxxxxxxxxxxxxxx", "sk_test_xxxxxxxxxxxxxxxxxxxx", "phc_xxxxxxxxxxxxxxxxxxxxxxxx", "AKIAIOSFODNN7EXAMPLE",
      "xoxb-xxxxxxxxxxxx", "-----BEGIN PRIVATE KEY-----", "0123456789abcdef0123456789abcdef0123456789abcdef",
      "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVowMTIzNDU2Nzg5", "your code is 123456", "from 203.0.113.7",
    ] : can(regex(re, v))])])
    error_message = "every blocked pattern compiles and masks at least one sample (an invalid RE2 pattern would match nothing)"
  }
  assert {
    condition = !anytrue([for v in [
      "GET /v1/conversations/{conversation}/approvals", "/v1/things/{id}", "acme", "mandate-core", "eve",
      "0b9d6c1e-3c2a-4f1e-9a7b-2d1f0e8c4b5a", "core/src/telemetry.rs", "approval.decided.v1", "0.33.1", "200", "dynamic/mandate-default",
      ] : anytrue([for re in yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.redaction.blocked_values : can(regex(re, v))])
    ])
    error_message = "routes, names, UUIDs, paths, versions and status codes are not masked"
  }
  assert {
    condition = (
      yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.transform.error_mode == "propagate"
      && yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.transform.trace_statements[0].statements[0] == "set(span.status.message, \"\")"
      && length(yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.transform.trace_statements[0].statements) == 1 + length(yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.redaction.blocked_values)
      && length(yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.transform.trace_statements[1].statements) == length(yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.redaction.blocked_values)
      && yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.transform.trace_statements[1].statements[0] == "replace_pattern(spanevent.name, \"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\\\.[A-Za-z]{2,}\", \"****\")"
    )
    error_message = "status messages are cleared; span and event names are masked with every blocked pattern; a failing statement drops the batch"
  }
  assert {
    condition = yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).exporters == {
      "otlp_http/posthog" = {
        traces_endpoint = "https://eu.i.posthog.com/i/v1/traces"
        logs_endpoint   = "https://eu.i.posthog.com/i/v1/logs"
        headers         = { Authorization = "Bearer $${env:POSTHOG_API_KEY}" }
      }
    }
    error_message = "one exporter: PostHog EU, the project key from the task secret (never in the config itself)"
  }
}

run "tail_sampling_keeps_errors_and_samples_the_rest" {
  command = apply
  variables {
    telemetry_enabled        = true
    telemetry_sample_percent = 25
  }

  assert {
    condition     = yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).service.pipelines.traces.processors == ["memory_limiter", "tail_sampling", "redaction", "transform", "batch"]
    error_message = "below 100% the traces pipeline samples first"
  }
  assert {
    condition = [for p in yamldecode(one([for e in jsondecode(aws_ecs_task_definition.collector[0].container_definitions)[0].environment : e.value if e.name == "OTELCOL_CONFIG"])).processors.tail_sampling.policies : p.type == "status_code" ? jsonencode(p.status_code) : jsonencode(p.probabilistic)] == [
      jsonencode({ status_codes = ["ERROR"] }), jsonencode({ sampling_percentage = 25 }),
    ]
    error_message = "every error trace is kept, and the given percentage of the rest"
  }
}

run "email_is_off_by_default_and_changes_nothing" {
  command = apply

  assert {
    condition = (
      length(aws_sesv2_configuration_set.main) == 0 && length(aws_sesv2_configuration_set_event_destination.feedback) == 0
      && length(aws_cloudwatch_event_rule.email_feedback) == 0 && length(aws_cloudwatch_event_target.email_feedback) == 0
      && length(aws_sqs_queue.email_feedback) == 0 && length(aws_sqs_queue.email_feedback_dead_letter) == 0
      && length(aws_sqs_queue_policy.email_feedback) == 0 && length(aws_iam_role_policy.core_email) == 0
      && length(aws_cloudwatch_metric_alarm.email_feedback_dlq) == 0
      && !contains(keys(aws_sqs_queue.consumer), "email") && !contains(keys(awscc_eventsv2_subscriber.consumer), "email")
    )
    error_message = "with email_enabled false there is no configuration set, feedback route, email consumer or SES permission"
  }
  assert {
    condition = !anytrue([
      for e in jsondecode(aws_ecs_task_definition.core.container_definitions)[0].environment : e.name == "MANDATE_QUEUE_EMAIL" || startswith(e.name, "MANDATE_EMAIL_")
    ])
    error_message = "no email env on the core while email is off"
  }
}

run "the_core_sends_only_as_the_domain_through_its_configuration_set" {
  command = apply
  variables {
    email_enabled = true
  }

  assert {
    condition = (
      keys(aws_sqs_queue.consumer) == ["agent-resume", "email", "posthog"]
      && jsondecode(awscc_eventsv2_subscriber.consumer["email"].filter_configuration.filters[0].pattern) == { "aws:DetailType" = ["approval.requested.v1"] }
      && jsondecode(aws_sqs_queue.consumer["email"].redrive_policy).maxReceiveCount == 20
      && keys(aws_cloudwatch_metric_alarm.dlq) == ["agent-resume", "email", "posthog"]
    )
    error_message = "the email consumer takes approval.requested.v1, rides out about 20 minutes of SES outage, and its DLQ alarms"
  }
  assert {
    condition = (
      keys(aws_cloudwatch_metric_alarm.consumer_stuck) == ["agent-resume", "email", "email-feedback"]
      && aws_cloudwatch_metric_alarm.consumer_stuck["email"].threshold == 20 * 60 + 300
      && aws_cloudwatch_metric_alarm.consumer_stuck["email"].dimensions == tomap({ QueueName = "mandate-dev-email.fifo" })
      && aws_cloudwatch_metric_alarm.consumer_stuck["email-feedback"].threshold == 900
      && aws_cloudwatch_metric_alarm.consumer_stuck["email-feedback"].dimensions == tomap({ QueueName = "mandate-dev-email-feedback" })
    )
    error_message = "the email queue alarms once its 20 retries (20 minutes) plus 5 are over, and the feedback queue after 15 minutes"
  }
  assert {
    condition = {
      for e in jsondecode(aws_ecs_task_definition.core.container_definitions)[0].environment : e.name => e.value
      if e.name == "MANDATE_QUEUE_EMAIL" || startswith(e.name, "MANDATE_EMAIL_")
      } == {
      MANDATE_QUEUE_EMAIL             = "https://sqs.ap-south-1.amazonaws.com/000000000000/mandate-dev-email.fifo"
      MANDATE_EMAIL_CONFIGURATION_SET = "mandate-dev"
      MANDATE_EMAIL_FEEDBACK_QUEUE    = "https://sqs.ap-south-1.amazonaws.com/000000000000/mandate-dev-email-feedback"
    }
    error_message = "the core gets the email queue, the configuration set and the feedback queue"
  }
  assert {
    condition = (
      aws_sesv2_configuration_set.main[0].configuration_set_name == "mandate-dev"
      && aws_sesv2_configuration_set.main[0].delivery_options[0].tls_policy == "REQUIRE"
      && aws_sesv2_configuration_set.main[0].reputation_options[0].reputation_metrics_enabled
      && aws_sesv2_configuration_set.main[0].sending_options[0].sending_enabled
    )
    error_message = "the configuration set requires TLS, tracks reputation and sends"
  }
  assert {
    condition = jsondecode(aws_iam_role_policy.core_email[0].policy).Statement == [
      {
        Effect   = "Allow"
        Action   = "ses:SendEmail"
        Resource = ["arn:aws:ses:ap-south-1:000000000000:identity/example.test", "arn:aws:ses:ap-south-1:000000000000:configuration-set/mandate-dev"]
      },
      {
        Effect   = "Allow"
        Action   = ["sqs:ReceiveMessage", "sqs:DeleteMessage", "sqs:ChangeMessageVisibility", "sqs:GetQueueAttributes"]
        Resource = "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-email-feedback"
      },
    ]
    error_message = "the core sends as the domain identity through its configuration set only, and reads the feedback queue"
  }
  assert {
    condition     = contains(one([for st in jsondecode(aws_iam_role_policy.core_events.policy).Statement : st.Resource if contains(flatten([st.Action]), "sqs:ReceiveMessage")]), "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-email.fifo")
    error_message = "the core reads the email consumer queue like every other consumer"
  }
  assert {
    condition = (
      aws_sesv2_configuration_set_event_destination.feedback[0].configuration_set_name == "mandate-dev"
      && aws_sesv2_configuration_set_event_destination.feedback[0].event_destination[0].enabled
      && aws_sesv2_configuration_set_event_destination.feedback[0].event_destination[0].matching_event_types == toset(["BOUNCE", "COMPLAINT"])
      && aws_sesv2_configuration_set_event_destination.feedback[0].event_destination[0].event_bridge_destination[0].event_bus_arn == "arn:aws:events:ap-south-1:000000000000:event-bus/default"
    )
    error_message = "bounces and complaints go to the default bus, the only one SES publishes to"
  }
  assert {
    condition = (
      aws_cloudwatch_event_rule.email_feedback[0].event_bus_name == "default"
      && jsondecode(aws_cloudwatch_event_rule.email_feedback[0].event_pattern) == {
        source        = ["aws.ses"]
        "detail-type" = ["Email Bounced", "Email Complaint Received"]
        detail        = { mail = { tags = { "ses:configuration-set" = ["mandate-dev"] } } }
      }
      && aws_cloudwatch_event_target.email_feedback[0].rule == aws_cloudwatch_event_rule.email_feedback[0].name
      && aws_cloudwatch_event_target.email_feedback[0].arn == "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-email-feedback"
    )
    error_message = "the rule takes this configuration set's bounces and complaints to the feedback queue"
  }
  assert {
    condition = (
      aws_sqs_queue_policy.email_feedback[0].queue_url == aws_sqs_queue.email_feedback[0].id
      && jsondecode(aws_sqs_queue_policy.email_feedback[0].policy).Statement == [{
        Effect    = "Allow"
        Principal = { Service = "events.amazonaws.com" }
        Action    = "sqs:SendMessage"
        Resource  = "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-email-feedback"
        Condition = { ArnEquals = { "aws:SourceArn" = "arn:aws:events:ap-south-1:000000000000:rule/mandate-dev-email-feedback" } }
      }]
    )
    error_message = "only the feedback rule may send to the feedback queue"
  }
  assert {
    condition = (
      !endswith(aws_sqs_queue.email_feedback[0].name, ".fifo") && aws_sqs_queue.email_feedback[0].sqs_managed_sse_enabled
      && aws_sqs_queue.email_feedback[0].visibility_timeout_seconds == 60
      && jsondecode(aws_sqs_queue.email_feedback[0].redrive_policy) == { deadLetterTargetArn = "arn:aws:sqs:ap-south-1:000000000000:mandate-dev-email-feedback-dlq", maxReceiveCount = 5 }
      && aws_sqs_queue.email_feedback_dead_letter[0].sqs_managed_sse_enabled && aws_sqs_queue.email_feedback_dead_letter[0].message_retention_seconds == 1209600
      && aws_cloudwatch_metric_alarm.email_feedback_dlq[0].dimensions == tomap({ QueueName = "mandate-dev-email-feedback-dlq" })
    )
    error_message = "an encrypted standard feedback queue with its own two-week dead-letter queue, which alarms"
  }
}
