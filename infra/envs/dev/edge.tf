locals {
  api_host = "api.${var.domain}"
}

# A remotely managed tunnel: cloudflared in the core's task dials out to Cloudflare, so nothing
# listens on the internet. The secret (and so the token) lives in tofu state (docs/deploy.md).
resource "random_bytes" "tunnel_secret" {
  length = 32
}

resource "cloudflare_zero_trust_tunnel_cloudflared" "api" {
  account_id    = var.cloudflare_account_id
  name          = local.name
  config_src    = "cloudflare"
  tunnel_secret = random_bytes.tunnel_secret.base64
}

data "cloudflare_zero_trust_tunnel_cloudflared_token" "api" {
  account_id = var.cloudflare_account_id
  tunnel_id  = cloudflare_zero_trust_tunnel_cloudflared.api.id
}

# Only the web API and the health check are public; /mcp is for eve over the private network.
resource "cloudflare_zero_trust_tunnel_cloudflared_config" "api" {
  account_id = var.cloudflare_account_id
  tunnel_id  = cloudflare_zero_trust_tunnel_cloudflared.api.id
  config = {
    ingress = [
      { hostname = local.api_host, path = "^/(v1/.*|healthz)$", service = "http://localhost:8080" },
      { service = "http_status:404" },
    ]
  }
}

resource "cloudflare_dns_record" "api" {
  zone_id = var.cloudflare_zone_id
  name    = local.api_host
  type    = "CNAME"
  content = "${cloudflare_zero_trust_tunnel_cloudflared.api.id}.cfargotunnel.com"
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
