# Credentials come from the environment: AWS_PROFILE=mumbai and CLOUDFLARE_API_TOKEN (runbook).
provider "aws" {
  region = var.region
  default_tags {
    tags = local.tags
  }
}

provider "awscc" {
  region = var.region
}

provider "cloudflare" {}
