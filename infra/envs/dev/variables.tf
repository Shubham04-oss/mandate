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

variable "ai_gateway_url" {
  type = string
}

# Model ids are configuration, never code (docs/design.md 8.6).
variable "model_default" {
  type = string
}

# Tag of the first task definitions; the deploy workflow registers new revisions per commit.
variable "image_tag" {
  type    = string
  default = "bootstrap"
}
