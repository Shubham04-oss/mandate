# Plan-time checks against mock providers: no AWS account or credentials involved.
mock_provider "aws" {
  mock_data "aws_availability_zones" {
    defaults = { names = ["ap-south-1a", "ap-south-1b", "ap-south-1c"] }
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
