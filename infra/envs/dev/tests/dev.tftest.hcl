# Plan-time checks against mock providers: no AWS or Cloudflare account or credentials involved.
# One file, because mock providers cannot be shared between test files.
mock_provider "aws" {
  mock_data "aws_availability_zones" {
    defaults = { names = ["ap-south-1a", "ap-south-1b"] }
  }
  mock_resource "aws_kms_key" {
    defaults = { arn = "arn:aws:kms:ap-south-1:000000000000:key/mock" }
  }
}
mock_provider "awscc" {}
mock_provider "cloudflare" {}

run "private_subnets_have_no_public_ips_and_route_through_nat" {
  command = plan

  assert {
    condition     = alltrue([for s in aws_subnet.private : !s.map_public_ip_on_launch])
    error_message = "private subnets must not hand out public IPs"
  }
  assert {
    condition     = length(aws_subnet.private) == 2 && length(aws_subnet.public) == 2
    error_message = "two AZs: Aurora and the ALB both need two subnets"
  }
  assert {
    condition     = aws_nat_gateway.egress.availability_mode == "regional"
    error_message = "one regional NAT gateway for egress"
  }
}

run "default_security_group_allows_nothing" {
  command = plan

  assert {
    condition     = length(aws_default_security_group.locked.ingress) == 0 && length(aws_default_security_group.locked.egress) == 0
    error_message = "the default security group must have no rules"
  }
}

run "endpoints_cover_what_tasks_need_without_nat" {
  command = plan

  assert {
    condition     = toset(keys(aws_vpc_endpoint.interface)) == toset(["ecr.api", "ecr.dkr", "logs", "secretsmanager"])
    error_message = "interface endpoints: ECR, logs, Secrets Manager"
  }
  assert {
    condition     = aws_vpc_endpoint.s3.vpc_endpoint_type == "Gateway"
    error_message = "S3 (ECR layers) through the free gateway endpoint"
  }
}

run "aurora_scales_to_zero_and_never_holds_a_password_in_state" {
  command = plan

  assert {
    condition     = aws_rds_cluster.main.serverlessv2_scaling_configuration[0].min_capacity == 0
    error_message = "dev Aurora scales to zero"
  }
  assert {
    condition     = aws_rds_cluster.main.manage_master_user_password == true && aws_rds_cluster.main.master_password == null
    error_message = "RDS manages the master password in Secrets Manager; it never enters state"
  }
  assert {
    condition     = aws_rds_cluster.main.storage_encrypted && aws_rds_cluster.main.deletion_protection
    error_message = "encrypted with the CMK and protected from deletion"
  }
  assert {
    condition     = alltrue([for i in aws_rds_cluster_instance.main : !i.publicly_accessible])
    error_message = "no public database instance"
  }
  assert {
    condition     = one([for p in aws_rds_cluster_parameter_group.main.parameter : p.value if p.name == "rds.force_ssl"]) == "1"
    error_message = "connections must use TLS"
  }
}

run "database_takes_no_traffic_from_addresses" {
  command = plan

  assert {
    condition     = length(aws_security_group.db.ingress) == 0
    error_message = "the database security group admits only other security groups, by rule"
  }
}

run "secrets_are_containers_under_the_cmk" {
  command = plan

  assert {
    condition = toset(keys(aws_secretsmanager_secret.this)) == toset([
      "core/database-url", "core/workos-api-key", "core/workos-client-id",
      "core/assertion-private-key", "core/event-signing-key",
      "migrate/database-url", "eve/database-url", "eve/ai-gateway-key", "eve/assertion-public-key",
    ])
    error_message = "one secret per value a task needs, named by who reads it"
  }
  assert {
    condition     = aws_kms_key.main.enable_key_rotation
    error_message = "the CMK rotates"
  }
}
