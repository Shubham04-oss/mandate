variable "region" {
  type    = string
  default = "ap-south-1"
}

variable "env" {
  type    = string
  default = "dev"
}

variable "vpc_cidr" {
  type    = string
  default = "10.40.0.0/16"
}

locals {
  name = "mandate-${var.env}"
  tags = {
    project    = "mandate"
    env        = var.env
    managed_by = "opentofu"
  }
}

# Check it is offered: `aws rds describe-db-engine-versions --engine postgres` (docs/deploy.md step 2).
variable "db_engine_version" {
  type    = string
  default = "17.11"
}

# db.t4g.small is the next step up if the core or eve outgrow it.
variable "db_instance_class" {
  type    = string
  default = "db.t4g.micro"
}

# Off only to tear dev down on purpose.
variable "db_deletion_protection" {
  type    = bool
  default = true
}

# Dev keeps a final snapshot by default; set true to destroy without one.
variable "db_skip_final_snapshot" {
  type    = bool
  default = false
}

# The Cloudflare zone's domain: the API is api.<domain>, the web app app.<domain>.
variable "domain" {
  type = string
}

variable "cloudflare_zone_id" {
  type = string
}

# The account that owns the zone; the tunnel is an account resource.
variable "cloudflare_account_id" {
  type = string
}

# Core and eve on Fargate Spot (about 70% cheaper); false puts them back on on-demand Fargate.
variable "use_fargate_spot" {
  type    = bool
  default = true
}

# MANDATE_READ_HOSTS: a data-egress allowlist (docs/security.md); list only hosts trusted with data.
variable "read_hosts" {
  type    = string
  default = ""
}

# MANDATE_AI_GATEWAY_URL: Cloudflare AI Gateway's OpenAI-compatible base; eve appends /chat/completions.
variable "ai_gateway_url" {
  type        = string
  description = "https://gateway.ai.cloudflare.com/v1/<account_id>/<gateway_id>/compat"
}

# Model ids are configuration, never code (docs/design.md 8.6).
variable "model_default" {
  type        = string
  description = "MANDATE_MODEL_DEFAULT as the gateway names it, e.g. dynamic/mandate-default"
}

# Tag of the first task definitions; the deploy workflow registers new revisions per commit.
variable "image_tag" {
  type    = string
  default = "bootstrap"
}

# The repository whose `dev` GitHub environment may assume the deploy role.
variable "github_repository" {
  type    = string
  default = "Shubham04-oss/mandate"
}

# GitHub's immutable subject (owner and repo ids), so a renamed or re-created repo cannot take the role.
# `gh api repos/OWNER/REPO/actions/oidc/customization/sub` prints it as sub_claim_prefix.
variable "github_oidc_sub_prefix" {
  type    = string
  default = "repo:Shubham04-oss@218535561/mandate@1466076280"
}

variable "github_oidc_provider_arn" {
  type        = string
  default     = ""
  description = "ARN of an existing GitHub OIDC provider in the account; empty creates one"
}

# Email for the alarms topic (alarms.tf); null creates no subscription. AWS emails a confirm link.
variable "alarm_email" {
  type    = string
  default = null
}

# The OpenTelemetry collector (ecs.tf) and the OTEL/PostHog env on the core and eve. Put a value in
# the posthog-key secret before turning it on (docs/deploy.md, Telemetry).
variable "telemetry_enabled" {
  type    = bool
  default = false
}

# Mounts the core's admin-only POST /v1/debug/error (and eve's), which forces a test error to check
# error tracking end to end. Off unless you are checking it (docs/deploy.md, Telemetry).
variable "debug_routes" {
  type    = bool
  default = false
}

# The core's own email through SES (email.tf): the email consumer, the configuration set, the bounce
# and complaint queue, and the core's env and IAM to send. The domain identity is verified outside
# tofu; the owner turns this on once it is (docs/deploy.md, Email).
variable "email_enabled" {
  type    = bool
  default = false
}

# Percent of traces without an error that the collector keeps; traces with an error are always kept.
# ponytail: tail sampling decides per trace, so every span must reach the same collector: below 100
# the collector must stay a single task (no load-balancing exporter tier in front of it).
variable "telemetry_sample_percent" {
  type    = number
  default = 100
  validation {
    condition     = var.telemetry_sample_percent > 0 && var.telemetry_sample_percent <= 100
    error_message = "telemetry_sample_percent is a percentage above 0 and at most 100"
  }
}

# Channels (docs/channels.md): the three channel consumers, the Slack, decision and proxy secrets on
# the core and eve, the channel env, and the core's runtime credential store. Put the secret values
# first (docs/deploy.md, Channels): a task does not start while a secret it reads is empty.
variable "channels_enabled" {
  type    = bool
  default = false
}

# MANDATE_SLACK_CLIENT_ID: Mandate's Slack app (Basic Information). Not secret; required with channels.
variable "slack_client_id" {
  type    = string
  default = ""
}

# WhatsApp and SMS through Twilio (stream E). Empty keeps Twilio unwired even with channels on, so
# Slack can go live before the Twilio secrets have values.
variable "twilio_account_sid" {
  type    = string
  default = ""
}

# MANDATE_WHATSAPP_FROM, e.g. whatsapp:+15551234567: the environment's one shared sender.
variable "whatsapp_from" {
  type    = string
  default = ""
}

# MANDATE_TWILIO_APPROVAL_TEMPLATE: the approved WhatsApp template's content SID (HX...) for cards
# outside the 24-hour customer window.
variable "twilio_approval_template" {
  type    = string
  default = ""
}
