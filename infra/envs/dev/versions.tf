terraform {
  required_version = ">= 1.10"

  # The bucket is passed at init (`-backend-config=bucket=...`) so no account-specific name is in git.
  backend "s3" {
    key          = "mandate/dev/terraform.tfstate"
    region       = "ap-south-1"
    encrypt      = true
    use_lockfile = true
  }

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.73"
    }
    # The EventBridge enhanced bus (eventsv2) exists only in the Cloud Control provider so far.
    awscc = {
      source  = "hashicorp/awscc"
      version = "~> 1.110"
    }
    cloudflare = {
      source  = "cloudflare/cloudflare"
      version = "~> 5.29"
    }
    random = {
      source  = "hashicorp/random"
      version = "~> 3.7"
    }
  }
}
