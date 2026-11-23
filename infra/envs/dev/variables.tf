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

# The repository whose `dev` GitHub environment may assume the deploy role.
variable "github_repository" {
  type    = string
  default = "Shubham04-oss/mandate"
}
