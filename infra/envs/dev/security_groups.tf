# Every rule names its peer; egress is explicit so a task reaches only what it uses. The tasks have
# public IPs (no NAT), so these groups are their only inbound guard: none admits the internet.
resource "aws_security_group" "core" {
  name        = "${local.name}-core"
  description = "The core: eve (MCP) in; HTTPS, the tunnel, Postgres and eve out"
  vpc_id      = aws_vpc.main.id
}

# The web API arrives through cloudflared on localhost, so eve is the only network peer.
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

# cloudflared dials Cloudflare's edge on 7844 (QUIC over UDP, falling back to HTTP/2 over TCP).
resource "aws_vpc_security_group_egress_rule" "core_to_tunnel" {
  for_each          = toset(["tcp", "udp"])
  security_group_id = aws_security_group.core.id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = each.key
  from_port         = 7844
  to_port           = 7844
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

resource "aws_security_group" "db_bootstrap" {
  name        = "${local.name}-db-bootstrap"
  description = "The one-off db-bootstrap task: nothing in; HTTPS and Postgres out"
  vpc_id      = aws_vpc.main.id
}

locals {
  task_security_groups = {
    core           = aws_security_group.core.id
    eve            = aws_security_group.eve.id
    migrate        = aws_security_group.migrate.id
    "db-bootstrap" = aws_security_group.db_bootstrap.id
  }
}

# HTTPS out over the public IP: AWS APIs (ECR, logs, Secrets Manager) for every task, WorkOS and
# AgentCore (core), the AI gateway (eve). ponytail: the one-off tasks get internet egress too, the
# price of no NAT and no endpoints; fine for dev, endpoints again if prod needs them sealed.
resource "aws_vpc_security_group_egress_rule" "https" {
  for_each          = local.task_security_groups
  security_group_id = each.value
  cidr_ipv4         = "0.0.0.0/0"
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

# The telemetry collector: the core and eve in on OTLP/HTTP; HTTPS (PostHog, AWS APIs) out, nothing else.
resource "aws_security_group" "collector" {
  count       = var.telemetry_enabled ? 1 : 0
  name        = "${local.name}-collector"
  description = "The OTel collector: the core and eve in (OTLP/HTTP); HTTPS out"
  vpc_id      = aws_vpc.main.id
}

locals {
  # The services that send telemetry, and so reach the collector.
  telemetry_senders = var.telemetry_enabled ? { core = aws_security_group.core.id, eve = aws_security_group.eve.id } : {}
}

resource "aws_vpc_security_group_ingress_rule" "collector" {
  for_each                     = local.telemetry_senders
  security_group_id            = aws_security_group.collector[0].id
  referenced_security_group_id = each.value
  ip_protocol                  = "tcp"
  from_port                    = 4318
  to_port                      = 4318
}

resource "aws_vpc_security_group_egress_rule" "to_collector" {
  for_each                     = local.telemetry_senders
  security_group_id            = each.value
  referenced_security_group_id = aws_security_group.collector[0].id
  ip_protocol                  = "tcp"
  from_port                    = 4318
  to_port                      = 4318
}

# Like the tasks' https rule: the public IP stands in for a NAT gateway (Docker Hub, logs, Secrets
# Manager, PostHog). Not in task_security_groups, so the collector gets no database rules.
resource "aws_vpc_security_group_egress_rule" "collector_https" {
  count             = var.telemetry_enabled ? 1 : 0
  security_group_id = aws_security_group.collector[0].id
  cidr_ipv4         = "0.0.0.0/0"
  ip_protocol       = "tcp"
  from_port         = 443
  to_port           = 443
}
