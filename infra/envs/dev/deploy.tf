# The deploy workflow's role: GitHub OIDC, no long-lived keys. It pushes images, runs the migrate
# task and rolls the two services; it cannot change infrastructure.
# An account has one GitHub OIDC provider; when another stack already made it, reuse it by ARN so
# this stack never owns, retags or destroys it.
resource "aws_iam_openid_connect_provider" "github" {
  count          = var.github_oidc_provider_arn == "" ? 1 : 0
  url            = "https://token.actions.githubusercontent.com"
  client_id_list = ["sts.amazonaws.com"]
}

locals {
  github_oidc_provider_arn = var.github_oidc_provider_arn != "" ? var.github_oidc_provider_arn : aws_iam_openid_connect_provider.github[0].arn
}

resource "aws_iam_role" "deploy" {
  name                 = "${local.name}-deploy"
  max_session_duration = 3600
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Federated = local.github_oidc_provider_arn }
      Action    = "sts:AssumeRoleWithWebIdentity"
      Condition = {
        StringEquals = {
          "token.actions.githubusercontent.com:aud" = "sts.amazonaws.com"
          "token.actions.githubusercontent.com:sub" = "repo:${var.github_repository}:environment:${var.env}"
          # Only the deploy workflow as it is on main, so a branch cannot ship its own steps.
          "token.actions.githubusercontent.com:job_workflow_ref" = "${var.github_repository}/.github/workflows/deploy.yml@refs/heads/main"
        }
      }
    }]
  })
}

locals {
  account_id      = data.aws_caller_identity.current.account_id
  task_def_prefix = "arn:aws:ecs:${var.region}:${local.account_id}:task-definition/${local.name}"
}

resource "aws_iam_role_policy" "deploy" {
  name = "deploy"
  role = aws_iam_role.deploy.name
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect   = "Allow"
        Action   = "ecr:GetAuthorizationToken"
        Resource = "*"
      },
      {
        Effect = "Allow"
        Action = [
          "ecr:BatchCheckLayerAvailability", "ecr:BatchGetImage", "ecr:CompleteLayerUpload",
          "ecr:InitiateLayerUpload", "ecr:PutImage", "ecr:UploadLayerPart", "ecr:DescribeImages",
        ]
        Resource = values(aws_ecr_repository.this)[*].arn
      },
      {
        # Neither action supports resource-level permissions.
        Effect   = "Allow"
        Action   = ["ecs:DescribeTaskDefinition", "ecs:RegisterTaskDefinition"]
        Resource = "*"
      },
      {
        Effect    = "Allow"
        Action    = "ecs:RunTask"
        Resource  = "${local.task_def_prefix}-migrate:*"
        Condition = { ArnEquals = { "ecs:cluster" = aws_ecs_cluster.main.arn } }
      },
      {
        Effect   = "Allow"
        Action   = "ecs:DescribeTasks"
        Resource = "arn:aws:ecs:${var.region}:${local.account_id}:task/${local.name}/*"
      },
      {
        Effect = "Allow"
        Action = ["ecs:DescribeServices", "ecs:UpdateService"]
        Resource = [
          "arn:aws:ecs:${var.region}:${local.account_id}:service/${local.name}/core",
          "arn:aws:ecs:${var.region}:${local.account_id}:service/${local.name}/eve",
        ]
      },
      {
        Effect    = "Allow"
        Action    = "iam:PassRole"
        Resource  = concat(values(aws_iam_role.execution)[*].arn, [aws_iam_role.core_task.arn])
        Condition = { StringEquals = { "iam:PassedToService" = "ecs-tasks.amazonaws.com" } }
      },
    ]
  })
}
