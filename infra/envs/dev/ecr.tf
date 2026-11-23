resource "aws_ecr_repository" "this" {
  for_each             = toset(["core", "eve"])
  name                 = "${local.name}/${each.key}"
  image_tag_mutability = "IMMUTABLE"
  image_scanning_configuration {
    scan_on_push = true
  }
}

resource "aws_ecr_lifecycle_policy" "this" {
  for_each   = aws_ecr_repository.this
  repository = each.value.name
  policy = jsonencode({
    rules = [
      {
        # Deploys push two images each; without this the mirrored db-bootstrap image would expire.
        rulePriority = 1
        description  = "keep the last 2 db-bootstrap images"
        selection    = { tagStatus = "tagged", tagPrefixList = ["db-bootstrap-"], countType = "imageCountMoreThan", countNumber = 2 }
        action       = { type = "expire" }
      },
      {
        rulePriority = 2
        description  = "keep the last 30 images"
        selection    = { tagStatus = "any", countType = "imageCountMoreThan", countNumber = 30 }
        action       = { type = "expire" }
      },
    ]
  })
}
