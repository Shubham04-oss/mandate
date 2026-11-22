# Deploy: AWS dev (ap-south-1)

The owner's runbook for the dev environment. Everything here is run by the owner, by hand, with
their own credentials: `infra/` is only ever validated and tested offline in CI, and the deploy
workflow does nothing until step 8 turns it on.

What it builds (`infra/envs/dev`, OpenTofu):

| Piece | What |
| --- | --- |
| Network | VPC, two AZs, private subnets behind one regional NAT gateway; endpoints for ECR, logs, Secrets Manager, S3 |
| Database | Aurora PostgreSQL serverless v2 (min capacity 0, but see Known limits), TLS forced, KMS-encrypted, RDS-managed master password |
| Secrets | Secrets Manager containers per task; values set by you (step 4), never in tofu state |
| Services | ECS Fargate: `core` behind an ALB that admits only Cloudflare, `eve` private and reachable only from the core, a one-off `migrate` task |
| Edge | Cloudflare: `api.<domain>` proxied to the ALB with a secret origin header, ACM certificate, WAF rate limit on `/v1/auth/*` |
| Events | EventBridge enhanced bus `mandate-dev`; per consumer a FIFO subscriber, SQS FIFO queue and DLQ (`agent-resume` first) |
| Tools | AgentCore browser (PUBLIC mode, recordings to S3), Bedrock guardrail |
| CI | IAM role for the deploy workflow through GitHub OIDC (no long-lived keys) |

Only `/v1/*` and `/healthz` are reachable from the internet; the MCP server (`/mcp`) answers only
eve over the private network.

## 0. You need

- AWS CLI with the `mumbai` profile (admin on the dev account), OpenTofu 1.10 or later, `jq`,
  `openssl`, `python3`, `gh`, Node (for Wrangler).
- A Cloudflare zone for the domain, and an API token for it with Zone DNS edit, Zone WAF edit and
  Workers edit.
- The WorkOS environment (API key and client id) and the AI gateway (URL, key, model id).

```sh
export AWS_PROFILE=mumbai AWS_REGION=ap-south-1
export CLOUDFLARE_API_TOKEN=...   # from your password manager; never commit it
```

## 1. State bucket (once)

```sh
ACCOUNT=$(aws sts get-caller-identity --query Account --output text)
BUCKET=mandate-tofu-state-$ACCOUNT
aws s3api create-bucket --bucket "$BUCKET" --create-bucket-configuration LocationConstraint=ap-south-1
aws s3api put-bucket-versioning --bucket "$BUCKET" --versioning-configuration Status=Enabled
aws s3api put-public-access-block --bucket "$BUCKET" --public-access-block-configuration \
  BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
aws s3api put-bucket-encryption --bucket "$BUCKET" --server-side-encryption-configuration \
  '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"}}]}'
```

Locking uses S3's native lock file (`use_lockfile`); no DynamoDB table.

## 2. Values

Create `infra/envs/dev/dev.tfvars` (ignored by git; nothing in it is secret, but it is yours):

```hcl
domain             = "example.com"        # your Cloudflare zone
cloudflare_zone_id = "..."
ai_gateway_url     = "https://..."
model_default      = "..."                # model id, as the gateway names it
read_hosts         = ""                   # MANDATE_READ_HOSTS: a data-egress allowlist, see docs/security.md
```

Check that `aurora_engine_version` (default `17.5`) is offered in ap-south-1 with auto-pause:
`aws rds describe-db-engine-versions --engine aurora-postgresql --query 'DBEngineVersions[].EngineVersion'`.

## 3. Apply

```sh
cd infra/envs/dev
tofu init -backend-config="bucket=$BUCKET"
tofu plan -var-file=dev.tfvars -out=dev.tfplan   # read it
tofu apply dev.tfplan
```

- If the account already has the GitHub OIDC provider, import it first:
  `tofu import -var-file=dev.tfvars aws_iam_openid_connect_provider.github arn:aws:iam::$ACCOUNT:oidc-provider/token.actions.githubusercontent.com`.
- If the zone already has an `http_ratelimit` or `http_request_late_transform` ruleset, fold its
  rules into `cloudflare_ruleset.auth_rate_limit` or `cloudflare_ruleset.origin_header`, or import
  it; a zone has one ruleset per phase.
- The services start on the tag `bootstrap`, which does not exist: ECS fails those first
  deployments and stops. The first deploy (step 9) brings them up.

## 4. Database roles

The cluster is private; create the roles through the RDS Data API with the master secret:

```sh
CLUSTER_ARN=$(aws rds describe-db-clusters --db-cluster-identifier mandate-dev --query 'DBClusters[0].DBClusterArn' --output text)
MASTER=$(tofu output -raw database_master_secret_arn)
sql() { aws rds-data execute-statement --resource-arn "$CLUSTER_ARN" --secret-arn "$MASTER" --database "$1" --sql "$2" >/dev/null; }
APP_PW=$(openssl rand -hex 24); EVE_PW=$(openssl rand -hex 24)
sql mandate "create role mandate_app login nosuperuser nobypassrls password '$APP_PW'"
sql mandate "create role mandate_eve login nosuperuser nobypassrls password '$EVE_PW'"
sql mandate "revoke all on schema public from public"
sql mandate "revoke connect on database mandate from public"
sql mandate "grant connect on database mandate to mandate_app"
sql mandate "create database eve"
sql eve     "grant create on database eve to mandate_eve"
sql eve     "grant usage, create on schema public to mandate_eve"
```

`mandate_app` gets its privileges from `core/sql/app_role.sql`, which the migrate task re-applies
after every migration, so a new table never runs without its grants. eve's workflow store (the
Postgres world: its own types in `public`, plus the `workflow` and `graphile_worker` schemas) lives
in its own database, `eve`, in the same cluster; `mandate_eve` cannot connect to `mandate` at all.
The eve image runs the world's migrations (`bootstrap`) before it starts. The migration role is the
cluster's master user (`mandate_migrator`).

## 5. Secrets

Set every value with the CLI (never through tofu, so nothing lands in state):

```sh
put() { aws secretsmanager put-secret-value --secret-id "mandate-dev/$1" --secret-string "$2" >/dev/null; }
HOST=$(tofu output -raw database_endpoint)
MIGRATOR_PW=$(aws secretsmanager get-secret-value --secret-id "$MASTER" --query SecretString --output text | jq -r .password)
enc() { python3 -c 'import sys, urllib.parse; print(urllib.parse.quote(sys.argv[1], safe=""))' "$1"; }

put core/database-url    "postgres://mandate_app:$APP_PW@$HOST:5432/mandate?sslmode=require"
put eve/database-url     "postgres://mandate_eve:$EVE_PW@$HOST:5432/eve?sslmode=require"
put migrate/database-url "postgres://mandate_migrator:$(enc "$MIGRATOR_PW")@$HOST:5432/mandate?sslmode=require"
put core/workos-api-key   "$WORKOS_API_KEY"
put core/workos-client-id "$WORKOS_CLIENT_ID"
put eve/ai-gateway-key    "$AI_GATEWAY_KEY"

umask 077; d=$(mktemp -d)
openssl genpkey -algorithm ed25519 -out "$d/assertion.pem"
openssl pkey -in "$d/assertion.pem" -pubout -out "$d/assertion.pub.pem"
openssl genpkey -algorithm ed25519 -out "$d/events.pem"
put core/assertion-private-key "$(cat "$d/assertion.pem")"
put eve/assertion-public-key   "$(cat "$d/assertion.pub.pem")"
put core/event-signing-key     "$(cat "$d/events.pem")"
put core/event-public-key      "$(openssl pkey -in "$d/events.pem" -pubout)"
rm -rf "$d"; unset APP_PW EVE_PW MIGRATOR_PW
```

If RDS rotates the master password, update `migrate/database-url` the same way. `sslmode=require`
encrypts but does not verify the server certificate; the traffic never leaves the VPC.

## 6. Cloudflare TLS

Set the zone's SSL/TLS mode to **Full (strict)** (or a configuration rule doing so for
`api.<domain>`): the ALB serves only HTTPS with an ACM certificate for `api.<domain>`.

## 7. GitHub

```sh
gh api -X PUT repos/Shubham04-oss/mandate/environments/dev
gh variable set AWS_DEPLOY_ROLE_ARN     --env dev --body "$(tofu output -raw deploy_role_arn)"
gh variable set MANDATE_MIGRATE_NETWORK --env dev --body "$(tofu output -raw migrate_network_configuration)"
```

The deploy role trusts only this repository's `dev` environment. Add required reviewers or a
branch rule (`main` only) to that environment in the repository settings if you want a gate.

## 8. Turn deploys on

```sh
gh variable set MANDATE_DEPLOY_ENABLED --body true   # a repository variable, not an environment one
```

From then on every push to `main` deploys. Set it to `false` (or delete it) to stop.

## 9. First deploy

Run the workflow by hand: `gh workflow run deploy.yml --ref main`, then `gh run watch`. It builds
and pushes the images (tagged with the commit), runs the migrate task (migrations, then
`core/sql/app_role.sql`) and fails if it exits non-zero, then rolls the core and eve (eve only once
`agents/` exists) and waits for them to be stable.

Each step takes the newest revision of the task definition and swaps in the new image, so after a
`tofu apply` that changes a task definition, run the workflow again to roll it out.

## 10. The web app

In `web/wrangler.jsonc`, replace `example.com` in the `dev` environment with your domain, then
from `web/`: `npx wrangler deploy --env dev`. Wrangler creates the `app.<domain>` custom domain.

## 11. Check

```sh
curl -s https://api.<domain>/healthz           # ok
curl -s -o /dev/null -w '%{http_code}\n' https://api.<domain>/mcp   # 404: not public
aws logs tail /mandate-dev/core --follow
```

Then the live tests the plan names (`bus_round_trip`, `guardrail_blocks_a_credential`, AgentCore)
with `AWS_PROFILE=mumbai` and the values from `tofu output`.

## Rollback

- **Code:** point the service at the previous revision:
  `aws ecs update-service --cluster mandate-dev --service core --task-definition mandate-dev-core:<previous>`
  (`aws ecs list-task-definitions --family-prefix mandate-dev-core --sort DESC`). Same for `eve`.
- **Migrations** only go forward. A bad migration is fixed by a new one, then a deploy.
- **Stop deploying:** `gh variable set MANDATE_DEPLOY_ENABLED --body false`.
- **Infrastructure:** revert the commit and `tofu apply`. The cluster has deletion protection;
  `tofu destroy` needs it turned off first and leaves a final snapshot.

## Origin secret

Cloudflare adds `X-Mandate-Origin-Auth` to every request for `api.<domain>` (a late transform
rule), and the ALB forwards only requests carrying it; anything else gets the 404. That keeps out
the ALB's own address and other Cloudflare zones, and is why the core trusts `CF-Connecting-IP` for
its per-IP sign-in limit (`MANDATE_TRUST_CF_IP=1`).

- The value is a `random_password` and lives in tofu state: the state bucket must stay private,
  versioned and encrypted (step 1), and only the owner may read it.
- Rotate: `tofu apply -var-file=dev.tfvars -replace=random_password.origin` (the same as
  `tofu taint random_password.origin`, then apply). Cloudflare and the ALB change in the same apply;
  requests in the seconds between the two get a 404.
- Upgrade path: per-hostname Authenticated Origin Pulls (mTLS from Cloudflare to the origin), with
  the ALB verifying the client certificate, so no shared secret is needed.

## Known limits (dev)

- One NAT gateway and interface endpoints in one AZ: cheap, not highly available.
- The origin is guarded by a shared secret header, not mTLS (see Origin secret).
- Aurora never auto-pauses: the core's outbox polls the database every second, so budget for
  0.5 ACU around the clock.
- eve reaches the core's MCP server, and the core reaches eve, over plain HTTP inside the VPC;
  the bearer assertion is short-lived.
- Server-sent events: Cloudflare closes a proxied response that sends nothing for 100 s, so the
  event stream needs keep-alives; the ALB idle timeout is 300 s.
- Recordings expire after 90 days and use SSE-S3; audit and recording retention are open
  (docs/design.md 11).
- The guardrail's personal-data list and denied topic are a starting point; tune them before
  real users.
