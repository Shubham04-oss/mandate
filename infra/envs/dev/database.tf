resource "aws_db_subnet_group" "main" {
  name       = local.name
  subnet_ids = aws_subnet.private[*].id
}

# Ingress rules name the task security groups that may connect (services.tf); no CIDRs.
resource "aws_security_group" "db" {
  name        = "${local.name}-db"
  description = "Aurora: Postgres from the core, eve and the migrate task only"
  vpc_id      = aws_vpc.main.id
}

resource "aws_rds_cluster_parameter_group" "main" {
  name   = local.name
  family = "aurora-postgresql17"
  parameter {
    name  = "rds.force_ssl"
    value = "1"
  }
}

resource "aws_rds_cluster" "main" {
  cluster_identifier              = local.name
  engine                          = "aurora-postgresql"
  engine_mode                     = "provisioned"
  engine_version                  = var.aurora_engine_version
  database_name                   = "mandate"
  master_username                 = "mandate_migrator"
  manage_master_user_password     = true
  master_user_secret_kms_key_id   = aws_kms_key.main.arn
  db_subnet_group_name            = aws_db_subnet_group.main.name
  vpc_security_group_ids          = [aws_security_group.db.id]
  db_cluster_parameter_group_name = aws_rds_cluster_parameter_group.main.name
  storage_encrypted               = true
  kms_key_id                      = aws_kms_key.main.arn
  backup_retention_period         = 1
  deletion_protection             = true
  final_snapshot_identifier       = "${local.name}-final"
  serverlessv2_scaling_configuration {
    min_capacity             = 0
    max_capacity             = 2
    seconds_until_auto_pause = 600
  }
}

resource "aws_rds_cluster_instance" "main" {
  count               = 1
  identifier          = "${local.name}-1"
  cluster_identifier  = aws_rds_cluster.main.id
  instance_class      = "db.serverless"
  engine              = aws_rds_cluster.main.engine
  engine_version      = aws_rds_cluster.main.engine_version
  publicly_accessible = false
}
