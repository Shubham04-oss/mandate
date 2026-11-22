# Every rule names its peer; egress is explicit so a task reaches only what it uses.
data "cloudflare_ip_ranges" "this" {}

resource "aws_security_group" "alb" {
  name        = "${local.name}-alb"
  description = "API load balancer: HTTPS from Cloudflare only"
  vpc_id      = aws_vpc.main.id
}

resource "aws_vpc_security_group_ingress_rule" "alb_from_cloudflare" {
  for_each          = toset(data.cloudflare_ip_ranges.this.ipv4_cidrs)
  security_group_id = aws_security_group.alb.id
  cidr_ipv4         = each.key
  ip_protocol       = "tcp"
  from_port         = 443
  to_port           = 443
}

resource "aws_vpc_security_group_egress_rule" "alb_to_core" {
  security_group_id            = aws_security_group.alb.id
  referenced_security_group_id = aws_security_group.core.id
  ip_protocol                  = "tcp"
  from_port                    = 8080
  to_port                      = 8080
}

resource "aws_security_group" "core" {
  name        = "${local.name}-core"
  description = "The core: the ALB (web API) and eve (MCP) in; HTTPS, Postgres and eve out"
  vpc_id      = aws_vpc.main.id
}

resource "aws_vpc_security_group_ingress_rule" "core_from_alb" {
  security_group_id            = aws_security_group.core.id
  referenced_security_group_id = aws_security_group.alb.id
  ip_protocol                  = "tcp"
  from_port                    = 8080
  to_port                      = 8080
}

resource "aws_vpc_security_group_ingress_rule" "core_from_eve" {
  security_group_id            = aws_security_group.core.id
  referenced_security_group_id = aws_security_group.eve.id
  ip_protocol                  = "tcp"
  from_port                    = 8080
  to_port                      = 8080
}

resource "aws_vpc_security_group_egress_rule" "core_to_eve" {
  security_group_id            = aws_security_group.core.id
  referenced_security_group_id = aws_security_group.eve.id
  ip_protocol                  = "tcp"
  from_port                    = 3000
  to_port                      = 3000
}

resource "aws_security_group" "eve" {
  name        = "${local.name}-eve"
  description = "eve: the core in; HTTPS, Postgres and the core out"
  vpc_id      = aws_vpc.main.id
}

resource "aws_vpc_security_group_ingress_rule" "eve_from_core" {
  security_group_id            = aws_security_group.eve.id
  referenced_security_group_id = aws_security_group.core.id
  ip_protocol                  = "tcp"
  from_port                    = 3000
  to_port                      = 3000
}

resource "aws_vpc_security_group_egress_rule" "eve_to_core" {
  security_group_id            = aws_security_group.eve.id
  referenced_security_group_id = aws_security_group.core.id
  ip_protocol                  = "tcp"
  from_port                    = 8080
  to_port                      = 8080
}

resource "aws_security_group" "migrate" {
  name        = "${local.name}-migrate"
  description = "The one-off migrate task: nothing in; HTTPS and Postgres out"
  vpc_id      = aws_vpc.main.id
}

locals {
  task_security_groups = {
    core    = aws_security_group.core.id
    eve     = aws_security_group.eve.id
    migrate = aws_security_group.migrate.id
  }
}

# HTTPS out: AWS APIs through the endpoints, WorkOS and AgentCore (core), the AI gateway (eve).
resource "aws_vpc_security_group_egress_rule" "https" {
  for_each          = { for k, v in local.task_security_groups : k => v if k != "migrate" }
  security_group_id = each.value
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "tcp"
  from_port         = 443
  to_port           = 443
}

# The migrate task talks to nothing on the internet: ECR, logs and secrets through the interface
# endpoints, image layers through the S3 gateway endpoint.
resource "aws_vpc_security_group_egress_rule" "migrate_to_endpoints" {
  security_group_id            = aws_security_group.migrate.id
  referenced_security_group_id = aws_security_group.endpoints.id
  ip_protocol                  = "tcp"
  from_port                    = 443
  to_port                      = 443
}

resource "aws_vpc_security_group_egress_rule" "migrate_to_s3" {
  security_group_id = aws_security_group.migrate.id
  prefix_list_id    = aws_vpc_endpoint.s3.prefix_list_id
  ip_protocol       = "tcp"
  from_port         = 443
  to_port           = 443
}

resource "aws_vpc_security_group_egress_rule" "to_db" {
  for_each                     = local.task_security_groups
  security_group_id            = each.value
  referenced_security_group_id = aws_security_group.db.id
  ip_protocol                  = "tcp"
  from_port                    = 5432
  to_port                      = 5432
}

resource "aws_vpc_security_group_ingress_rule" "db" {
  for_each                     = local.task_security_groups
  security_group_id            = aws_security_group.db.id
  referenced_security_group_id = each.value
  ip_protocol                  = "tcp"
  from_port                    = 5432
  to_port                      = 5432
}
