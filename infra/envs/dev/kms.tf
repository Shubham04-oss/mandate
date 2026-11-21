# One key for the secrets and the database; the default key policy delegates to IAM.
resource "aws_kms_key" "main" {
  description             = "${local.name} secrets and database"
  enable_key_rotation     = true
  deletion_window_in_days = 30
}

resource "aws_kms_alias" "main" {
  name          = "alias/${local.name}"
  target_key_id = aws_kms_key.main.key_id
}
