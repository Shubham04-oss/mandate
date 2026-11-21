resource "aws_ecs_cluster" "main" {
  name = local.name
}

resource "aws_cloudwatch_log_group" "this" {
  for_each          = toset(["core", "eve", "migrate"])
  name              = "/${local.name}/${each.key}"
  retention_in_days = 30
}

# Private names: eve.<ns> for the core, core.<ns> for eve's MCP connection.
resource "aws_service_discovery_private_dns_namespace" "main" {
  name = "${local.name}.internal"
  vpc  = aws_vpc.main.id
}

resource "aws_service_discovery_service" "this" {
  for_each = toset(["core", "eve"])
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
  internal = aws_service_discovery_private_dns_namespace.main.name
  # Secret env names per task; the entrypoint turns *_PEM values into 0600 files.
  secret_env = {
    "core/database-url"          = "DATABASE_URL"
    "core/workos-api-key"        = "WORKOS_API_KEY"
    "core/workos-client-id"      = "WORKOS_CLIENT_ID"
    "core/assertion-private-key" = "MANDATE_ASSERTION_PRIVATE_KEY_PEM"
    "core/event-signing-key"     = "MANDATE_EVENT_SIGNING_KEY_PEM"
    "migrate/database-url"       = "MANDATE_MIGRATION_DATABASE_URL"
    "eve/database-url"           = "WORKFLOW_POSTGRES_URL"
    "eve/ai-gateway-key"         = "AI_GATEWAY_API_KEY"
    "eve/assertion-public-key"   = "MANDATE_ASSERTION_PUBLIC_KEY_PEM"
  }
  core_environment = {
    AWS_REGION         = var.region
    MANDATE_READ_HOSTS = var.read_hosts
    MANDATE_WEB_ORIGIN = "https://app.${var.domain}"
    MANDATE_EVE_URL    = "http://eve.${local.internal}:3000"
    MANDATE_EVENT_BUS  = awscc_eventsv2_event_bus.main.event_bus_arn
    # One MANDATE_QUEUE_<NAME> per consumer (lane F's consumer kit).
    MANDATE_QUEUE_AGENT_RESUME = aws_sqs_queue.consumer["agent-resume"].url
  }
  eve_environment = {
    PORT                   = "3000"
    MANDATE_CORE_MCP_URL   = "http://core.${local.internal}:8080/mcp"
    MANDATE_AI_GATEWAY_URL = var.ai_gateway_url
    MANDATE_MODEL_DEFAULT  = var.model_default
  }
}

locals {
  container = {
    for task in ["core", "eve", "migrate"] : task => {
      name      = task
      essential = true
      image     = "${aws_ecr_repository.this[task == "eve" ? "eve" : "core"].repository_url}:${task == "migrate" ? "migrate-" : ""}${var.image_tag}"
      secrets   = [for k, arn in local.task_secrets[task] : { name = local.secret_env[k], valueFrom = arn }]
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
  container_definitions = jsonencode([merge(local.container["core"], {
    portMappings = [{ containerPort = 8080, protocol = "tcp" }]
    environment  = [for k, v in local.core_environment : { name = k, value = v }]
  })])
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
  container_definitions = jsonencode([local.container["migrate"]])
}

resource "aws_ecs_service" "core" {
  name                   = "core"
  cluster                = aws_ecs_cluster.main.id
  task_definition        = aws_ecs_task_definition.core.arn
  desired_count          = 1
  launch_type            = "FARGATE"
  enable_execute_command = false
  network_configuration {
    subnets          = aws_subnet.private[*].id
    security_groups  = [aws_security_group.core.id]
    assign_public_ip = false
  }
  load_balancer {
    target_group_arn = aws_lb_target_group.core.arn
    container_name   = "core"
    container_port   = 8080
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
  depends_on = [aws_lb_listener.https]
}

resource "aws_ecs_service" "eve" {
  name                   = "eve"
  cluster                = aws_ecs_cluster.main.id
  task_definition        = aws_ecs_task_definition.eve.arn
  desired_count          = 1
  launch_type            = "FARGATE"
  enable_execute_command = false
  network_configuration {
    subnets          = aws_subnet.private[*].id
    security_groups  = [aws_security_group.eve.id]
    assign_public_ip = false
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
}
