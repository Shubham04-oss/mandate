resource "aws_ecs_cluster" "main" {
  name = local.name
}

resource "aws_ecs_cluster_capacity_providers" "main" {
  cluster_name       = aws_ecs_cluster.main.name
  capacity_providers = ["FARGATE", "FARGATE_SPOT"]
}

resource "aws_cloudwatch_log_group" "this" {
  for_each          = toset(concat(["core", "eve", "migrate", "db-bootstrap"], var.telemetry_enabled ? ["collector"] : []))
  name              = "/${local.name}/${each.key}"
  retention_in_days = 30
}

# Private names: eve.<ns> for the core, core.<ns> for eve's MCP connection, collector.<ns> for OTLP.
resource "aws_service_discovery_private_dns_namespace" "main" {
  name = "${local.name}.internal"
  vpc  = aws_vpc.main.id
}

resource "aws_service_discovery_service" "this" {
  for_each = toset(concat(["core", "eve"], var.telemetry_enabled ? ["collector"] : []))
  name     = each.key
  dns_config {
    namespace_id   = aws_service_discovery_private_dns_namespace.main.id
    routing_policy = "MULTIVALUE"
    dns_records {
      type = "A"
      ttl  = 10
    }
  }
}

locals {
  # cloudflare/cloudflared:2026.10.7, multi-arch index; ECS picks the linux/amd64 image.
  cloudflared_image = "docker.io/cloudflare/cloudflared@sha256:4955524aeaa379e154023d5a027c250f7b35041a2e7e950cf13f144bc9b9c5ba"
  # postgres:17-alpine (17.12), linux/amd64 manifest; bump with the mirror step in docs/deploy.md.
  db_bootstrap_image_digest = "sha256:e80acae3f1109e8e4533618b594918936f2858c63afc7204c4773148a673add8"
  # The RDS root CAs for ap-south-1 (truststore.pki.rds.amazonaws.com); tests pin its SHA-256.
  # Every database client writes it to a file and connects with sslmode=verify-full.
  rds_ca_pem = file("${path.module}/rds-ca/ap-south-1-bundle.crt")
  internal   = aws_service_discovery_private_dns_namespace.main.name
  # otel/opentelemetry-collector-contrib:0.166.0, multi-arch index; ECS picks the linux/amd64 image.
  collector_image = "docker.io/otel/opentelemetry-collector-contrib@sha256:3e18fe04181de50bdd72e23ad4a4d6214b54351e63d496f1c58127cc4541ad7f"
  # PostHog EU Cloud: OTLP from the collector, the core's /batch events.
  posthog_host = "https://eu.i.posthog.com"
  # OTLP/HTTP with protobuf to the collector; the SDKs append /v1/traces and /v1/logs.
  otel_environment = var.telemetry_enabled ? {
    OTEL_EXPORTER_OTLP_ENDPOINT = "http://collector.${local.internal}:4318"
    OTEL_EXPORTER_OTLP_PROTOCOL = "http/protobuf"
  } : {}
  # Secret env names per task. The core entrypoint turns *_PEM values into 0600 files; eve reads
  # MANDATE_ASSERTION_PUBLIC_KEY as PEM text (lane D).
  secret_env = {
    "core/database-url"          = "DATABASE_URL"
    "core/workos-api-key"        = "WORKOS_API_KEY"
    "core/workos-client-id"      = "WORKOS_CLIENT_ID"
    "core/assertion-private-key" = "MANDATE_ASSERTION_PRIVATE_KEY_PEM"
    "core/event-signing-key"     = "MANDATE_EVENT_SIGNING_KEY_PEM"
    "core/event-public-key"      = "MANDATE_EVENT_PUBLIC_KEY_PEM"
    "eve/database-url"           = "WORKFLOW_POSTGRES_URL"
    "eve/ai-gateway-key"         = "MANDATE_AI_GATEWAY_KEY" # Cloudflare AI Gateway token (AI Gateway Run)
    "eve/assertion-public-key"   = "MANDATE_ASSERTION_PUBLIC_KEY"
    "posthog-key"                = "POSTHOG_API_KEY" # the core's, only while telemetry is on (iam.tf)
  }
  core_environment = merge({
    RDS_CA_PEM         = local.rds_ca_pem
    AWS_REGION         = var.region
    MANDATE_READ_HOSTS = var.read_hosts
    MANDATE_WEB_ORIGIN = "https://app.${var.domain}"
    MANDATE_API_ORIGIN = "https://${local.api_host}"
    # The web API arrives only through cloudflared on localhost, which sets CF-Connecting-IP; the
    # core honours it only from a loopback peer, so eve (which reaches :8080 directly) cannot forge it.
    MANDATE_TRUST_CF_IP = "1"
    MANDATE_EVE_URL     = "http://eve.${local.internal}:3000"
    MANDATE_EVENT_BUS   = awscc_eventsv2_event_bus.main.event_bus_arn
    # One MANDATE_QUEUE_<NAME> per consumer (lane F's consumer kit).
    MANDATE_QUEUE_AGENT_RESUME = aws_sqs_queue.consumer["agent-resume"].url
    MANDATE_QUEUE_POSTHOG      = aws_sqs_queue.consumer["posthog"].url
    MANDATE_BROWSER_ID         = aws_bedrockagentcore_browser.main.browser_id
    MANDATE_GUARDRAIL_ID       = aws_bedrock_guardrail.main.guardrail_id
    MANDATE_GUARDRAIL_VERSION  = aws_bedrock_guardrail_version.main.version
    }, local.otel_environment, var.telemetry_enabled ? {
    OTEL_SERVICE_NAME = "mandate-core"
    POSTHOG_HOST      = local.posthog_host
  } : {})
  eve_environment = merge({
    RDS_CA_PEM             = local.rds_ca_pem
    PORT                   = "3000"
    MANDATE_CORE_MCP_URL   = "http://core.${local.internal}:8080/mcp"
    MANDATE_AI_GATEWAY_URL = var.ai_gateway_url
    MANDATE_MODEL_DEFAULT  = var.model_default
  }, local.otel_environment, var.telemetry_enabled ? { OTEL_SERVICE_NAME = "eve" } : {})
}

locals {
  container = {
    for task in ["core", "eve", "migrate"] : task => {
      name      = task
      essential = true
      image     = "${aws_ecr_repository.this[task == "eve" ? "eve" : "core"].repository_url}:${task == "migrate" ? "migrate-" : ""}${var.image_tag}"
      # migrate reads the master password live from the RDS-managed secret, so rotation never stales it.
      secrets = concat([for k, arn in local.task_secrets[task] : { name = local.secret_env[k], valueFrom = arn }],
      task == "migrate" ? [{ name = "PGPASSWORD", valueFrom = "${local.db_master_secret_arn}:password::" }] : [])
      logConfiguration = {
        logDriver = "awslogs"
        options = {
          awslogs-group         = aws_cloudwatch_log_group.this[task].name
          awslogs-region        = var.region
          awslogs-stream-prefix = task
        }
      }
    }
  }
}

resource "aws_ecs_task_definition" "core" {
  family                   = "${local.name}-core"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = 256
  memory                   = 512
  execution_role_arn       = aws_iam_role.execution["core"].arn
  task_role_arn            = aws_iam_role.core_task.arn
  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "X86_64"
  }
  # The core stays first: the deploy workflow swaps the image of containerDefinitions[0].
  container_definitions = jsonencode([merge(local.container["core"], {
    portMappings = [{ containerPort = 8080, protocol = "tcp" }]
    environment  = [for k, v in local.core_environment : { name = k, value = v }]
    # The image has no curl: the binary probes its own /healthz on loopback.
    healthCheck = {
      command     = ["CMD", "/usr/local/bin/mandate", "healthcheck"]
      interval    = 10
      timeout     = 5
      retries     = 3
      startPeriod = 30
    }
    }), {
    name      = "cloudflared"
    essential = true
    image     = local.cloudflared_image
    # The image's own nonroot user, stated so a retagged image cannot run as root.
    user    = "65532:65532"
    command = ["tunnel", "--no-autoupdate", "run"]
    # Register with Cloudflare only once the core answers, so the edge never routes to a cold core.
    dependsOn = [{ containerName = "core", condition = "HEALTHY" }]
    secrets   = [{ name = "TUNNEL_TOKEN", valueFrom = aws_secretsmanager_secret.tunnel_token.arn }]
    logConfiguration = {
      logDriver = "awslogs"
      options = {
        awslogs-group         = aws_cloudwatch_log_group.this["core"].name
        awslogs-region        = var.region
        awslogs-stream-prefix = "cloudflared"
      }
    }
  }])
}

resource "aws_ecs_task_definition" "eve" {
  family                   = "${local.name}-eve"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = 256
  memory                   = 1024
  execution_role_arn       = aws_iam_role.execution["eve"].arn
  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "X86_64"
  }
  container_definitions = jsonencode([merge(local.container["eve"], {
    portMappings = [{ containerPort = 3000, protocol = "tcp" }]
    environment  = [for k, v in local.eve_environment : { name = k, value = v }]
    # eve's public health route on loopback; the image has no curl, so node fetches it.
    healthCheck = {
      command     = ["CMD", "node", "-e", "fetch('http://127.0.0.1:3000/eve/v1/health').then(r => process.exit(r.ok ? 0 : 1), () => process.exit(1))"]
      interval    = 10
      timeout     = 5
      retries     = 3
      startPeriod = 30
    }
  })])
}

# Run by the deploy workflow before the services roll: migrate, then re-apply the app role grants.
resource "aws_ecs_task_definition" "migrate" {
  family                   = "${local.name}-migrate"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = 256
  memory                   = 512
  execution_role_arn       = aws_iam_role.execution["migrate"].arn
  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "X86_64"
  }
  container_definitions = jsonencode([merge(local.container["migrate"], {
    environment = [for k, v in {
      RDS_CA_PEM = local.rds_ca_pem
      # No password: sqlx and psql take it from PGPASSWORD.
      MANDATE_MIGRATION_DATABASE_URL = "postgres://${aws_db_instance.main.username}@${aws_db_instance.main.address}:5432/${aws_db_instance.main.db_name}?sslmode=verify-full&sslrootcert=/run/mandate/rds-ca.pem"
    } : { name = k, value = v }]
  })])
}

# One-off, run by the owner (docs/deploy.md step 4): creates the app and eve roles and eve's
# database. Role passwords come from the URLs already in their secrets, so none is in state or logs.
resource "aws_ecs_task_definition" "db_bootstrap" {
  family                   = "${local.name}-db-bootstrap"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = 256
  memory                   = 512
  execution_role_arn       = aws_iam_role.db_bootstrap_execution.arn
  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "X86_64"
  }
  container_definitions = jsonencode([{
    name      = "db-bootstrap"
    essential = true
    # The image's unprivileged user; /tmp stays writable for the CA file.
    user = "postgres"
    # Pinned by digest: the official postgres image mirrored into the core repository (deploy.md step 4).
    image   = "${aws_ecr_repository.this["core"].repository_url}@${local.db_bootstrap_image_digest}"
    command = ["sh", "-c", "printf '%s\\n' \"$RDS_CA_PEM\" > /tmp/rds-ca.pem && printf '%s\\n' \"$BOOTSTRAP_SQL\" | psql -v ON_ERROR_STOP=1 -f -"]
    environment = [for k, v in {
      PGHOST        = aws_db_instance.main.address
      PGUSER        = aws_db_instance.main.username
      PGDATABASE    = aws_db_instance.main.db_name
      PGSSLMODE     = "verify-full"
      PGSSLROOTCERT = "/tmp/rds-ca.pem"
      RDS_CA_PEM    = local.rds_ca_pem
      BOOTSTRAP_SQL = file("${path.module}/sql/db_bootstrap.sql")
    } : { name = k, value = v }]
    secrets = [
      { name = "PGPASSWORD", valueFrom = "${local.db_master_secret_arn}:password::" },
      { name = "APP_DATABASE_URL", valueFrom = aws_secretsmanager_secret.this["core/database-url"].arn },
      { name = "EVE_DATABASE_URL", valueFrom = aws_secretsmanager_secret.this["eve/database-url"].arn },
    ]
    logConfiguration = {
      logDriver = "awslogs"
      options = {
        awslogs-group         = aws_cloudwatch_log_group.this["db-bootstrap"].name
        awslogs-region        = var.region
        awslogs-stream-prefix = "db-bootstrap"
      }
    }
  }])
}

resource "aws_ecs_service" "core" {
  name                   = "core"
  cluster                = aws_ecs_cluster.main.id
  task_definition        = aws_ecs_task_definition.core.arn
  desired_count          = 1
  enable_execute_command = false
  # A Spot interruption only stops the task; the service starts a new one.
  capacity_provider_strategy {
    capacity_provider = var.use_fargate_spot ? "FARGATE_SPOT" : "FARGATE"
    weight            = 1
  }
  # Public subnets and a public IP instead of a NAT gateway; the security group admits no internet.
  network_configuration {
    subnets          = aws_subnet.public[*].id
    security_groups  = [aws_security_group.core.id]
    assign_public_ip = true
  }
  service_registries {
    registry_arn = aws_service_discovery_service.this["core"].arn
  }
  deployment_circuit_breaker {
    enable   = true
    rollback = true
  }
  # The deploy workflow rolls new revisions; tofu owns everything else.
  lifecycle {
    ignore_changes = [task_definition]
  }
  depends_on = [aws_ecs_cluster_capacity_providers.main]
}

resource "aws_ecs_service" "eve" {
  name                   = "eve"
  cluster                = aws_ecs_cluster.main.id
  task_definition        = aws_ecs_task_definition.eve.arn
  desired_count          = 1
  enable_execute_command = false
  # A Spot interruption only stops the task; the service starts a new one.
  capacity_provider_strategy {
    capacity_provider = var.use_fargate_spot ? "FARGATE_SPOT" : "FARGATE"
    weight            = 1
  }
  # Public subnets and a public IP instead of a NAT gateway; the security group admits no internet.
  network_configuration {
    subnets          = aws_subnet.public[*].id
    security_groups  = [aws_security_group.eve.id]
    assign_public_ip = true
  }
  service_registries {
    registry_arn = aws_service_discovery_service.this["eve"].arn
  }
  deployment_circuit_breaker {
    enable   = true
    rollback = true
  }
  lifecycle {
    ignore_changes = [task_definition]
  }
  depends_on = [aws_ecs_cluster_capacity_providers.main]
}

# The telemetry collector (variables.tf telemetry_enabled): the single funnel to PostHog, which
# drops what may be sensitive (collector/config.yaml.tftpl) before anything leaves.
locals {
  # Attribute keys that may leave; everything else is removed. Ids, routes, codes, types: no text.
  # Not otel.status_description, url.*, db.statement or server.address: they can carry user input.
  collector_allowed_keys = [
    "service.name", "service.version", "service.namespace", "service.instance.id",
    "deployment.environment", "deployment.environment.name",
    "telemetry.sdk.name", "telemetry.sdk.language", "telemetry.sdk.version",
    "process.runtime.name", "process.runtime.version", "host.arch", "os.type",
    "cloud.provider", "cloud.platform", "cloud.region",
    "otel.scope.name", "otel.scope.version", "otel.status_code",
    "http.request.method", "http.route", "http.response.status_code", "http.method", "http.status_code",
    "url.scheme", "network.protocol.name", "network.protocol.version", "server.port",
    "rpc.system", "rpc.service", "rpc.method", "rpc.grpc.status_code", "rpc.jsonrpc.version", "rpc.jsonrpc.error_code",
    "db.system", "db.system.name", "db.name", "db.namespace", "db.operation", "db.operation.name",
    "db.collection.name", "db.sql.table",
    "error.type", "exception.type",
    "code.filepath", "code.lineno", "code.namespace", "code.function",
    "code.file.path", "code.line.number", "code.function.name", "thread.id", "thread.name", "level", "target",
    "gen_ai.system", "gen_ai.provider.name", "gen_ai.operation.name", "gen_ai.request.model", "gen_ai.response.model",
    "gen_ai.usage.input_tokens", "gen_ai.usage.output_tokens", "gen_ai.response.finish_reasons", "gen_ai.tool.name",
    "tenant.id", "agent.id", "conversation.id",
  ]
  # RE2 (Go regexp, as tofu's regex()): any match in a kept value, log body or span name is masked.
  collector_blocked_values = [
    "[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\\.[A-Za-z]{2,}",                   # email addresses
    "eyJ[A-Za-z0-9_=-]{10,}(\\.[A-Za-z0-9_=-]*)*",                       # JWTs (base64 of '{"')
    "(?i)bearer\\s+[A-Za-z0-9._~+/=-]+",                                 # Authorization header values
    "\\b(sk|pk|rk)[-_][A-Za-z0-9_-]{16,}",                               # sk-..., sk_live_... API keys
    "\\b(phc|phx|phs|ghp|gho|ghs|ghu|ghr|github_pat)_[A-Za-z0-9_]{16,}", # PostHog, GitHub tokens
    "\\bxox[abprs]-[A-Za-z0-9-]{10,}",                                   # Slack tokens
    "\\b(AKIA|ASIA)[0-9A-Z]{16}\\b",                                     # AWS access key ids
    "-----BEGIN[A-Z ]*-----",                                            # PEM headers (the body is base64)
    "\\b[0-9a-fA-F]{32,}\\b",                                            # long hex: keys, hex passwords
    "[A-Za-z0-9+/]{40,}={0,2}",                                          # long base64
    "[A-Za-z0-9_-]{40,}",                                                # long base64url
    "\\b[0-9]{6}\\b",                                                    # 6-digit sign-in codes
    "\\b([0-9]{1,3}\\.){3}[0-9]{1,3}\\b",                                # IPv4 addresses (client IPs)
  ]
  collector_config = templatefile("${path.module}/collector/config.yaml.tftpl", {
    posthog_host   = local.posthog_host
    sample_percent = var.telemetry_sample_percent
    allowed_keys   = local.collector_allowed_keys
    blocked_values = local.collector_blocked_values
    # OTTL strings unescape like JSON, so jsonencode quotes each pattern.
    span_statements       = concat(["set(span.status.message, \"\")"], [for re in local.collector_blocked_values : "replace_pattern(span.name, ${jsonencode(re)}, \"****\")"])
    span_event_statements = [for re in local.collector_blocked_values : "replace_pattern(spanevent.name, ${jsonencode(re)}, \"****\")"]
  })
}

resource "aws_ecs_task_definition" "collector" {
  count                    = var.telemetry_enabled ? 1 : 0
  family                   = "${local.name}-collector"
  requires_compatibilities = ["FARGATE"]
  network_mode             = "awsvpc"
  cpu                      = 256
  memory                   = 512
  execution_role_arn       = aws_iam_role.collector_execution[0].arn
  runtime_platform {
    operating_system_family = "LINUX"
    cpu_architecture        = "X86_64"
  }
  # No healthCheck: the image is FROM scratch (no shell, no curl) and the binary has no probe
  # subcommand; ECS restarts the task if the collector exits.
  container_definitions = jsonencode([{
    name      = "collector"
    essential = true
    image     = local.collector_image
    # The image's own nonroot user, stated so a retagged image cannot run as root.
    user         = "10001:10001"
    command      = ["--config=env:OTELCOL_CONFIG"]
    portMappings = [{ containerPort = 4318, protocol = "tcp" }]
    environment  = [{ name = "OTELCOL_CONFIG", value = local.collector_config }]
    secrets      = [{ name = "POSTHOG_API_KEY", valueFrom = aws_secretsmanager_secret.this["posthog-key"].arn }]
    logConfiguration = {
      logDriver = "awslogs"
      options = {
        awslogs-group         = aws_cloudwatch_log_group.this["collector"].name
        awslogs-region        = var.region
        awslogs-stream-prefix = "collector"
      }
    }
  }])
}

resource "aws_ecs_service" "collector" {
  count                  = var.telemetry_enabled ? 1 : 0
  name                   = "collector"
  cluster                = aws_ecs_cluster.main.id
  task_definition        = aws_ecs_task_definition.collector[0].arn
  desired_count          = 1
  enable_execute_command = false
  # A Spot interruption only stops the task; the service starts a new one.
  capacity_provider_strategy {
    capacity_provider = var.use_fargate_spot ? "FARGATE_SPOT" : "FARGATE"
    weight            = 1
  }
  # Public subnets and a public IP instead of a NAT gateway; the security group admits no internet.
  network_configuration {
    subnets          = aws_subnet.public[*].id
    security_groups  = [aws_security_group.collector[0].id]
    assign_public_ip = true
  }
  service_registries {
    registry_arn = aws_service_discovery_service.this["collector"].arn
  }
  deployment_circuit_breaker {
    enable   = true
    rollback = true
  }
  depends_on = [aws_ecs_cluster_capacity_providers.main]
}
