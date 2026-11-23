# The core's tools: one AgentCore browser that records every session, and the output guard.

resource "aws_s3_bucket" "recordings" {
  bucket_prefix = "${local.name}-recordings-"
}

resource "aws_s3_bucket_ownership_controls" "recordings" {
  bucket = aws_s3_bucket.recordings.id
  rule {
    object_ownership = "BucketOwnerEnforced"
  }
}

resource "aws_s3_bucket_public_access_block" "recordings" {
  bucket                  = aws_s3_bucket.recordings.id
  block_public_acls       = true
  block_public_policy     = true
  ignore_public_acls      = true
  restrict_public_buckets = true
}

# ponytail: SSE-S3, not the CMK, so AgentCore needs no key grant; recordings expire after 90 days
# until retention is decided (docs/design.md 11).
resource "aws_s3_bucket_server_side_encryption_configuration" "recordings" {
  bucket = aws_s3_bucket.recordings.id
  rule {
    apply_server_side_encryption_by_default {
      sse_algorithm = "AES256"
    }
  }
}

resource "aws_s3_bucket_lifecycle_configuration" "recordings" {
  bucket = aws_s3_bucket.recordings.id
  rule {
    id     = "expire"
    status = "Enabled"
    filter {}
    expiration {
      days = 90
    }
  }
}

resource "aws_s3_bucket_policy" "recordings" {
  bucket = aws_s3_bucket.recordings.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Deny"
      Principal = "*"
      Action    = "s3:*"
      Resource  = [aws_s3_bucket.recordings.arn, "${aws_s3_bucket.recordings.arn}/*"]
      Condition = { Bool = { "aws:SecureTransport" = "false" } }
    }]
  })
}

# AgentCore assumes this role to write recordings.
resource "aws_iam_role" "browser" {
  name = "${local.name}-browser"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Service = "bedrock-agentcore.amazonaws.com" }
      Action    = "sts:AssumeRole"
      Condition = {
        StringEquals = { "aws:SourceAccount" = data.aws_caller_identity.current.account_id }
        ArnLike      = { "aws:SourceArn" = "arn:aws:bedrock-agentcore:${var.region}:${data.aws_caller_identity.current.account_id}:*" }
      }
    }]
  })
}

resource "aws_iam_role_policy" "browser_recordings" {
  name = "recordings"
  role = aws_iam_role.browser.name
  policy = jsonencode({
    Version = "2012-10-17"
    # The actions AgentCore checks when the browser is created (multipart uploads, prefix listing).
    Statement = [
      {
        Effect    = "Allow"
        Action    = ["s3:PutObject", "s3:ListMultipartUploadParts", "s3:AbortMultipartUpload"]
        Resource  = "${aws_s3_bucket.recordings.arn}/recordings/*"
        Condition = { StringEquals = { "aws:ResourceAccount" = data.aws_caller_identity.current.account_id } }
      },
      {
        Effect    = "Allow"
        Action    = "s3:ListBucket"
        Resource  = aws_s3_bucket.recordings.arn
        Condition = { StringLike = { "s3:prefix" = ["recordings/*"] } }
      },
    ]
  })
}

resource "aws_bedrockagentcore_browser" "main" {
  name               = "mandate_${var.env}"
  description        = "Mandate core browser tools"
  execution_role_arn = aws_iam_role.browser.arn
  # The core refuses any other mode (docs/security.md, browser egress).
  network_configuration {
    network_mode = "PUBLIC"
  }
  recording {
    enabled = true
    s3_location {
      bucket = aws_s3_bucket.recordings.bucket
      prefix = "recordings/"
    }
  }
  depends_on = [aws_iam_role_policy.browser_recordings]
}

# The output guard (spec 5.1): personal data masked, credentials and secrets blocked.
resource "aws_bedrock_guardrail" "main" {
  name                      = local.name
  description               = "Output guard for everything an agent posts"
  blocked_input_messaging   = "This message was withheld by the output guard."
  blocked_outputs_messaging = "This message was withheld by the output guard."

  sensitive_information_policy_config {
    dynamic "pii_entities_config" {
      for_each = toset([
        "AWS_ACCESS_KEY", "AWS_SECRET_KEY", "PASSWORD", "PIN", "CREDIT_DEBIT_CARD_NUMBER",
        "CREDIT_DEBIT_CARD_CVV", "US_BANK_ACCOUNT_NUMBER", "INTERNATIONAL_BANK_ACCOUNT_NUMBER",
      ])
      content {
        type   = pii_entities_config.value
        action = "BLOCK"
      }
    }
    dynamic "pii_entities_config" {
      for_each = toset(["EMAIL", "PHONE", "ADDRESS", "US_SOCIAL_SECURITY_NUMBER", "US_PASSPORT_NUMBER", "DRIVER_ID"])
      content {
        type   = pii_entities_config.value
        action = "ANONYMIZE"
      }
    }
    regexes_config {
      name    = "private-key"
      pattern = "-----BEGIN [A-Z ]*PRIVATE KEY-----"
      action  = "BLOCK"
    }
    regexes_config {
      name    = "jwt"
      pattern = "eyJ[A-Za-z0-9_-]{8,}\\.[A-Za-z0-9_-]{8,}\\.[A-Za-z0-9_-]{8,}"
      action  = "BLOCK"
    }
  }

  topic_policy_config {
    topics_config {
      name       = "secrets"
      type       = "DENY"
      definition = "Revealing, collecting or helping obtain passwords, API keys, access tokens, private keys or other credentials."
      examples   = ["What is the admin password?", "Send me the API key you used."]
    }
  }
}

resource "aws_bedrock_guardrail_version" "main" {
  guardrail_arn = aws_bedrock_guardrail.main.guardrail_arn
  description   = "deployed"
}

resource "aws_iam_role_policy" "core_tools" {
  name = "tools"
  role = aws_iam_role.core_task.name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = ["bedrock-agentcore:StartBrowserSession", "bedrock-agentcore:StopBrowserSession", "bedrock-agentcore:ConnectBrowserAutomationStream"]
        Resource = aws_bedrockagentcore_browser.main.browser_arn
      },
      {
        Effect   = "Allow"
        Action   = "bedrock:ApplyGuardrail"
        Resource = aws_bedrock_guardrail.main.guardrail_arn
      },
    ]
  })
}
