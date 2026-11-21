locals {
  api_host = "api.${var.domain}"
}

resource "aws_acm_certificate" "api" {
  domain_name       = local.api_host
  validation_method = "DNS"
  lifecycle {
    create_before_destroy = true
  }
}

resource "cloudflare_dns_record" "api_validation" {
  for_each = {
    for o in aws_acm_certificate.api.domain_validation_options : o.domain_name => o
  }
  zone_id = var.cloudflare_zone_id
  name    = trimsuffix(each.value.resource_record_name, ".")
  type    = each.value.resource_record_type
  content = trimsuffix(each.value.resource_record_value, ".")
  ttl     = 60
  proxied = false
}

resource "aws_acm_certificate_validation" "api" {
  certificate_arn         = aws_acm_certificate.api.arn
  validation_record_fqdns = [for r in cloudflare_dns_record.api_validation : r.name]
}

resource "aws_lb" "api" {
  name                       = "${local.name}-api"
  load_balancer_type         = "application"
  internal                   = false
  subnets                    = aws_subnet.public[*].id
  security_groups            = [aws_security_group.alb.id]
  drop_invalid_header_fields = true
  # Server-sent events from /v1/conversations/*/events stay open between messages.
  idle_timeout = 300
}

resource "aws_lb_target_group" "core" {
  name                 = "${local.name}-core"
  port                 = 8080
  protocol             = "HTTP"
  target_type          = "ip"
  vpc_id               = aws_vpc.main.id
  deregistration_delay = 30
  health_check {
    path    = "/healthz"
    matcher = "200"
  }
}

resource "aws_lb_listener" "https" {
  load_balancer_arn = aws_lb.api.arn
  port              = 443
  protocol          = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = aws_acm_certificate_validation.api.certificate_arn
  default_action {
    type = "fixed-response"
    fixed_response {
      content_type = "application/json"
      status_code  = "404"
      message_body = jsonencode({ error = { code = "not_found", message = "Not found." } })
    }
  }
}

# Only the web API and the health check are public; /mcp is for eve over the private network.
resource "aws_lb_listener_rule" "api" {
  listener_arn = aws_lb_listener.https.arn
  priority     = 10
  action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.core.arn
  }
  condition {
    host_header {
      values = [local.api_host]
    }
  }
  condition {
    path_pattern {
      values = ["/v1/*", "/healthz"]
    }
  }
}

resource "cloudflare_dns_record" "api" {
  zone_id = var.cloudflare_zone_id
  name    = local.api_host
  type    = "CNAME"
  content = aws_lb.api.dns_name
  ttl     = 1
  proxied = true
}

# A coarse edge limit in front of the core's own per-email and per-IP limits. The values fit every
# Cloudflare plan (the free plan allows only a 10 s period and timeout, keyed on IP and colo).
resource "cloudflare_ruleset" "auth_rate_limit" {
  zone_id = var.cloudflare_zone_id
  name    = "${local.name} auth rate limit"
  kind    = "zone"
  phase   = "http_ratelimit"
  rules = [{
    description = "Sign-in endpoints"
    expression  = "(http.request.uri.path wildcard \"/v1/auth/*\")"
    action      = "block"
    enabled     = true
    ratelimit = {
      characteristics     = ["ip.src", "cf.colo.id"]
      period              = 10
      requests_per_period = 20
      mitigation_timeout  = 10
    }
  }]
}
