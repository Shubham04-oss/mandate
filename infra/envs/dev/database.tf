resource "aws_db_subnet_group" "main" {
  name       = local.name
  subnet_ids = aws_subnet.private[*].id
}

# Ingress rules name the task security groups that may connect (security_groups.tf); no CIDRs.
resource "aws_security_group" "db" {
  name        = "${local.name}-db"
  description = "RDS: Postgres from the core, eve, migrate and db-bootstrap tasks only"
  vpc_id      = aws_vpc.main.id
}

resource "aws_db_parameter_group" "main" {
  name   = local.name
  family = "postgres17"
  parameter {
    name  = "rds.force_ssl"
    value = "1"
  }
}

# A full destroy and re-apply gets a new suffix, so the next final snapshot never clashes with this one.
resource "random_id" "db_final_snapshot" {
  byte_length = 4
}

resource "aws_db_instance" "main" {
  identifier     = local.name
  engine         = "postgres"
  engine_version = var.db_engine_version
  # Minor upgrades go through var.db_engine_version, so state never drifts from what runs.
  auto_minor_version_upgrade    = false
  instance_class                = var.db_instance_class
  storage_type                  = "gp3"
  allocated_storage             = 20
  max_allocated_storage         = 100
  db_name                       = "mandate"
  username                      = "mandate_migrator"
  manage_master_user_password   = true
  master_user_secret_kms_key_id = aws_kms_key.main.arn
  db_subnet_group_name          = aws_db_subnet_group.main.name
  vpc_security_group_ids        = [aws_security_group.db.id]
  parameter_group_name          = aws_db_parameter_group.main.name
  publicly_accessible           = false
  multi_az                      = false
  storage_encrypted             = true
  kms_key_id                    = aws_kms_key.main.arn
  backup_retention_period       = 7
  deletion_protection           = var.db_deletion_protection
  skip_final_snapshot           = var.db_skip_final_snapshot
  final_snapshot_identifier     = "${local.name}-final-${random_id.db_final_snapshot.hex}"
  # Pinned so a change of RDS default CA cannot break verify-full (rds-ca/ holds this root).
  ca_cert_identifier = "rds-ca-rsa2048-g1"
}
