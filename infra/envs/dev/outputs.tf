# Values the runbook (docs/deploy.md) and the deploy workflow's repository variables need.
# The one-off tasks run like the services: public subnets and a public IP, no NAT.
output "deploy_role_arn" {
  value = aws_iam_role.deploy.arn
}

output "migrate_network_configuration" {
  description = "Value for the MANDATE_MIGRATE_NETWORK repository variable"
  value = jsonencode({
    awsvpcConfiguration = {
      subnets        = aws_subnet.public[*].id
      securityGroups = [aws_security_group.migrate.id]
      assignPublicIp = "ENABLED"
    }
  })
}

output "db_bootstrap_network_configuration" {
  description = "--network-configuration for the one-off db-bootstrap task (docs/deploy.md step 4)"
  value = jsonencode({
    awsvpcConfiguration = {
      subnets        = aws_subnet.public[*].id
      securityGroups = [aws_security_group.db_bootstrap.id]
      assignPublicIp = "ENABLED"
    }
  })
}

output "ecr_repositories" {
  value = { for k, r in aws_ecr_repository.this : k => r.repository_url }
}

# Host only (no port): the runbook builds the URLs with :5432.
output "database_endpoint" {
  value = aws_db_instance.main.address
}

output "database_master_secret_arn" {
  description = "RDS-managed secret holding the migration role's password"
  value       = one(aws_db_instance.main.master_user_secret[*].secret_arn)
}

output "secret_names" {
  value = { for k, s in aws_secretsmanager_secret.this : k => s.name }
}

output "api_url" {
  value = "https://${local.api_host}"
}

output "event_bus_arn" {
  value = awscc_eventsv2_event_bus.main.event_bus_arn
}

output "browser_id" {
  value = aws_bedrockagentcore_browser.main.browser_id
}

output "guardrail" {
  value = { id = aws_bedrock_guardrail.main.guardrail_id, version = aws_bedrock_guardrail_version.main.version }
}

output "recordings_bucket" {
  value = aws_s3_bucket.recordings.bucket
}
