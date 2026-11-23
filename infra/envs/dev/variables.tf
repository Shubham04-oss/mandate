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

# Needs a version with auto-pause (min capacity 0); check `aws rds describe-db-engine-versions`.
variable "aurora_engine_version" {
  type    = string
  default = "17.5"
}

# The Cloudflare zone's domain: the API is api.<domain>, the web app app.<domain>.
variable "domain" {
  type = string
}

variable "cloudflare_zone_id" {
  type = string
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
