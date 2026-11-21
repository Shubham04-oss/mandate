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
