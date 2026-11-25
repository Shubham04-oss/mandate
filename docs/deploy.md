# Deploy: AWS dev (ap-south-1)

The owner's runbook for the dev environment. Everything here is run by the owner, by hand, with
their own credentials: `infra/` is only ever validated and tested offline in CI, and the deploy
workflow does nothing until step 8 turns it on.

What it builds (`infra/envs/dev`, OpenTofu):

| Piece | What |
| --- | --- |
| Network | VPC, two AZs; the tasks in public subnets with public IPs and no inbound from the internet (no NAT gateway, no VPC endpoints); the database in private subnets with no route out |
| Database | RDS PostgreSQL 17, one `db.t4g.micro` (single-AZ, gp3 20 GB growing to 100 GB), private, TLS forced, KMS-encrypted, RDS-managed master password, 7-day backups |
| Secrets | Secrets Manager containers per task; values set by you (step 4), never in tofu state. The one exception is the tunnel token, which tofu writes (see Tunnel token) |
| Services | ECS on Fargate Spot (`use_fargate_spot`, default true): `core` with a `cloudflared` sidecar, `eve` reachable only from the core; one-off `migrate` and `db-bootstrap` tasks on on-demand Fargate |
| Edge | Cloudflare: a remotely managed tunnel; `api.<domain>` is a proxied CNAME to it, the sidecar forwards to the core on `localhost:8080`; WAF rate limit on `/v1/auth/*` |
| Events | EventBridge enhanced bus `mandate-dev`; per consumer a FIFO subscriber, SQS FIFO queue and DLQ (`agent-resume` first) |
| Tools | AgentCore browser (PUBLIC mode, recordings to S3), Bedrock guardrail |
| CI | IAM role for the deploy workflow through GitHub OIDC (no long-lived keys) |

Only `/v1/*` and `/healthz` are reachable from the internet (the tunnel's ingress rules; anything
else is a 404 at Cloudflare); the MCP server (`/mcp`) answers only eve over the private network.
Nothing listens on a public address: the core's security group admits only eve.

## 0. You need

- AWS CLI with the `mumbai` profile (admin on the dev account), OpenTofu 1.10 or later, `jq`,
  `openssl`, `python3`, `gh`, Node (for Wrangler).
- A Cloudflare zone for the domain, and an API token with Zone DNS edit, Zone WAF edit, Workers
  edit and Account Cloudflare Tunnel edit (the tunnel is an account resource).
- The WorkOS environment (API key and client id) and a Cloudflare AI Gateway with the provider keys
  stored in it (BYOK), a dynamic route for the default model, and a token with AI Gateway Run.

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
cloudflare_account_id = "..."             # the account that owns the zone (dashboard, zone overview)
ai_gateway_url     = "https://gateway.ai.cloudflare.com/v1/<account_id>/<gateway_id>/compat"
model_default      = "dynamic/mandate-default" # a gateway dynamic route, or provider/model
read_hosts         = ""                   # MANDATE_READ_HOSTS: a data-egress allowlist, see docs/security.md
```

Check that `db_engine_version` (default `17.11`) is offered in ap-south-1:
`aws rds describe-db-engine-versions --engine postgres --query 'DBEngineVersions[].EngineVersion'`.
`db_instance_class` defaults to `db.t4g.micro`; set `db.t4g.small` here if dev outgrows it.

## 3. Apply

```sh
cd infra/envs/dev
tofu init -backend-config="bucket=$BUCKET"
tofu plan -var-file=dev.tfvars -out=dev.tfplan   # read it
tofu apply dev.tfplan
```

- If the account already has the GitHub OIDC provider (another stack made it), set
  `github_oidc_provider_arn = "arn:aws:iam::<ACCOUNT>:oidc-provider/token.actions.githubusercontent.com"`
  in `dev.tfvars`. This stack then uses it without owning it; do not import it.
- If the zone already has an `http_ratelimit` ruleset, fold its rules into
  `cloudflare_ruleset.auth_rate_limit`, or import it; a zone has one ruleset per phase.
- The services start on the tag `bootstrap`, which does not exist: ECS fails those first
  deployments and stops. The first deploy (step 9) brings them up.

## 4. Database roles

The database is private and has no Data API. Mirror the bootstrap image once, put the two role
URLs, then run the one-off `db-bootstrap` task: it logs in as the
master user with the RDS-managed secret over `verify-full` TLS, reads each role's password from its
own URL secret, creates `mandate_app`, `mandate_eve` and the `eve` database, and exits. No password
is printed, logged or in tofu state; you generate them here and they go straight into Secrets Manager.

The task, like `migrate`, now runs in a public subnet with HTTPS egress (dev has no NAT and no
endpoints), so it could pull from ECR Public directly; it still pulls a mirror from the core
repository, so the image that holds the master password comes from this account. Copy the
official `postgres:17-alpine` linux/amd64 image, by the digest `ecs.tf` pins
(`db_bootstrap_image_digest`), into the core repository once. A single-manifest
`imagetools create` copies it unchanged, so the digest stays the same; if it ever differed, the task
would fail to pull rather than run something else.

```sh
REGISTRY=$ACCOUNT.dkr.ecr.ap-south-1.amazonaws.com
DIGEST=sha256:e80acae3f1109e8e4533618b594918936f2858c63afc7204c4773148a673add8   # = db_bootstrap_image_digest
aws ecr get-login-password | docker login --username AWS --password-stdin "$REGISTRY"
docker buildx imagetools create --tag "$REGISTRY/mandate-dev/core:db-bootstrap-postgres-17.12" \
  "public.ecr.aws/docker/library/postgres@$DIGEST"
docker buildx imagetools inspect "$REGISTRY/mandate-dev/core:db-bootstrap-postgres-17.12" | grep Digest   # = $DIGEST
```

Tags are immutable, so `imagetools create` fails if the tag already exists: if `inspect` shows the
pinned digest there, the mirror is already done; otherwise use a new tag. The repository's lifecycle
policy keeps the last two `db-bootstrap-*` images apart from the deploy images. The deploy role can
push to this repository and so could evict the mirror (a denial of service only: the digest pin
stops a substitute from running); mirror it again if a pull fails. To move to a newer postgres,
update the digest in `ecs.tf`, mirror it under a new tag, and `tofu apply`.

```sh
put() { aws secretsmanager put-secret-value --secret-id "mandate-dev/$1" --secret-string "$2" >/dev/null; }
HOST=$(tofu output -raw database_endpoint)
APP_PW=$(openssl rand -hex 24); EVE_PW=$(openssl rand -hex 24)   # required: lowercase hex, 32+ characters

put core/database-url    "postgres://mandate_app:$APP_PW@$HOST:5432/mandate?sslmode=verify-full&sslrootcert=/run/mandate/rds-ca.pem"
put eve/database-url     "postgres://mandate_eve:$EVE_PW@$HOST:5432/eve?sslmode=verify-full&sslrootcert=/tmp/rds-ca.pem"
unset APP_PW EVE_PW

TASK=$(aws ecs run-task --cluster mandate-dev --launch-type FARGATE --task-definition mandate-dev-db-bootstrap \
  --network-configuration "$(tofu output -raw db_bootstrap_network_configuration)" \
  --query 'tasks[0].taskArn' --output text)
aws ecs wait tasks-stopped --cluster mandate-dev --tasks "$TASK"
aws ecs describe-tasks --cluster mandate-dev --tasks "$TASK" \
  --query 'tasks[0].[stopCode, stoppedReason, containers[0].exitCode]' --output text   # exit code 0
aws logs tail /mandate-dev/db-bootstrap --since 1h   # CREATE ROLE, CREATE DATABASE, GRANT
```

The SQL is `infra/envs/dev/sql/db_bootstrap.sql`. It is safe to rerun: a rerun resets each role's
password to what its URL secret holds, so rotating one is `put` a new URL, rerun the task, then
redeploy the service that uses it. A non-zero exit with nothing in the log usually means the image
pull failed (not mirrored, or a different digest); `stoppedReason` says why. The task refuses a
password that is not lowercase hex of 32 or more characters, before any role statement runs.
A role statement that fails can put its password in the RDS server log (log exports are off, so it
stays inside RDS); if one fails, fix the cause, `put` a URL with a new password, and rerun.

`mandate_app` gets its privileges from `core/sql/app_role.sql`, which the migrate task re-applies
after every migration, so a new table never runs without its grants. eve's workflow store (the
Postgres world: its own types in `public`, plus the `workflow` and `graphile_worker` schemas) lives
in its own database, `eve`, on the same instance. `PUBLIC` has no rights on either database or its
`public` schema: only `mandate_app` connects to `mandate`, only `mandate_eve` to `eve`.
The eve image runs the world's migrations (`bootstrap`) before it starts. The migration role is the
instance's master user (`mandate_migrator`): the migrate task takes its password live from the
RDS-managed secret (`PGPASSWORD`) and its URL, without a password, from the task definition, so the
7-day rotation of that secret needs nothing from you.

## 5. Secrets

Set the rest with the CLI (never through tofu, so nothing lands in state); the database URLs are
already in from step 4:

```sh
put core/workos-api-key   "$WORKOS_API_KEY"
put core/workos-client-id "$WORKOS_CLIENT_ID"
put eve/ai-gateway-key    "$AI_GATEWAY_KEY"   # Cloudflare AI Gateway token (AI Gateway Run)

umask 077; d=$(mktemp -d)
openssl genpkey -algorithm ed25519 -out "$d/assertion.pem"
openssl pkey -in "$d/assertion.pem" -pubout -out "$d/assertion.pub.pem"
openssl genpkey -algorithm ed25519 -out "$d/events.pem"
put core/assertion-private-key "$(cat "$d/assertion.pem")"
put eve/assertion-public-key   "$(cat "$d/assertion.pub.pem")"
put core/event-signing-key     "$(cat "$d/events.pem")"
put core/event-public-key      "$(openssl pkey -in "$d/events.pem" -pubout)"
rm -rf "$d"
```

Every database client uses `sslmode=verify-full` against the RDS root CAs for ap-south-1
(`infra/envs/dev/rds-ca/ap-south-1-bundle.crt`, from truststore.pki.rds.amazonaws.com; the tests pin
its SHA-256). The task definitions pass it as `RDS_CA_PEM` and each container writes it to the file
its URL's `sslrootcert` names (the core and migrate: `/run/mandate/rds-ca.pem`, eve:
`/tmp/rds-ca.pem`). If RDS adds a new root CA, fetch the bundle again, check it, and update the hash.

## 6. Cloudflare Tunnel

Nothing to set by hand: the apply created the tunnel, its ingress rules, the `api.<domain>` CNAME
and the token secret. The zone's SSL/TLS mode does not apply to tunnel traffic (cloudflared's
connection to Cloudflare is always encrypted). Once the first deploy (step 9) runs, the tunnel shows
as healthy under Zero Trust, Networks, Tunnels, and `aws logs tail /mandate-dev/core
--log-stream-name-prefix cloudflared` shows its registered connections.

## 7. GitHub

```sh
gh variable set AWS_DEPLOY_ROLE_ARN     --env dev --body "$(tofu output -raw deploy_role_arn)"
gh variable set MANDATE_MIGRATE_NETWORK --env dev --body "$(tofu output -raw migrate_network_configuration)"
```

The deploy role trusts only `.github/workflows/deploy.yml` as it is on `main`, running in this
repository's `dev` environment. Also restrict the environment to protected branches (required;
`main` is protected), so no other branch can run a job in `dev` at all:

```sh
gh api -X PUT repos/Shubham04-oss/mandate/environments/dev \
  -F 'deployment_branch_policy[protected_branches]=true' -F 'deployment_branch_policy[custom_branch_policies]=false'
```

Add required reviewers there too if you want a manual gate.

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
`tofu apply` that changes a task definition, run the workflow again to roll it out. A rerun of the
same commit skips images already pushed (tags are immutable). If the circuit breaker rolls a service
back, the job fails, naming the revision still running.

## 10. The web app

In `web/wrangler.jsonc`, replace `example.com` in the `dev` environment with your domain, then from
`web/`:

```sh
npm ci
CLOUDFLARE_ENV=dev VITE_MANDATE_API_URL=https://api.<domain> npm run build
npx wrangler deploy
```

Wrangler creates the `app.<domain>` custom domain. Both values are fixed when the app is built:
Cloudflare's Vite plugin picks the `dev` environment at build time (`--env` on `wrangler deploy` is
ignored for this build), and the core's address is compiled into the browser code. A build without
`VITE_MANDATE_API_URL` fails on purpose, so a deployed app never shows the made-up stand-in data
used in `npm run dev`.

## 11. Check

```sh
curl -s https://api.<domain>/healthz           # ok
curl -s -o /dev/null -w '%{http_code}\n' https://api.<domain>/mcp   # 404: not public (the tunnel's catch-all)
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
- **Infrastructure:** revert the commit and `tofu apply`. The database has deletion protection;
  `tofu destroy` needs `db_deletion_protection = false` applied first and leaves a final snapshot
  (`db_skip_final_snapshot = true` to skip it).

## Tunnel token

`cloudflared` in the core's task connects out to Cloudflare with the tunnel token (from the
`mandate-dev/cloudflared/tunnel-token` secret, as `TUNNEL_TOKEN`); Cloudflare sends `api.<domain>`
through it to the core on `localhost:8080` and sets `CF-Connecting-IP`, which is why the core
trusts that header for its per-IP sign-in limit (`MANDATE_TRUST_CF_IP=1`), and only from a loopback
peer, so eve cannot forge it. cloudflared starts only once the core's health check
(`mandate healthcheck`, a GET of its own `/healthz`) passes.

- Tofu creates the tunnel, so its secret (`random_bytes.tunnel_secret`) and token are in tofu
  state, and tofu writes the token into Secrets Manager: the state bucket must stay private,
  versioned and encrypted (step 1), and only the owner may read it. Anyone with the token can serve
  `api.<domain>`.
- Rotate: `tofu apply -var-file=dev.tfvars -replace=random_bytes.tunnel_secret`. That replaces the
  tunnel (new id, new token, the CNAME follows) and the secret value; then run the deploy workflow
  (step 9) so the core restarts with the new token. The API is down between the apply and the roll.

## Known limits (dev)

- No NAT gateway and no VPC endpoints: every task, the one-off `migrate` and `db-bootstrap`
  included, has a public IP and HTTPS egress to the internet. Inbound is closed by the security
  groups; the database stays in private subnets with no route out.
- Core and eve run on Fargate Spot: an interruption stops the task and the service starts another,
  so expect the odd minute of downtime. `use_fargate_spot = false` switches back (it replaces
  the services).
- One tunnel connector (one core task): a restart drops the API until cloudflared reconnects.
- One single-AZ `db.t4g.micro`: no failover, and an AZ outage or maintenance takes the database
  down.
- eve reaches the core's MCP server, and the core reaches eve, over plain HTTP inside the VPC;
  the bearer assertion is short-lived.
- Server-sent events: Cloudflare closes a proxied response that sends nothing for 100 s, so the
  event stream needs keep-alives.
- Recordings expire after 90 days and use SSE-S3; audit and recording retention are open
  (docs/design.md 11).
- The guardrail's personal-data list and denied topic are a starting point; tune them before
  real users.

## Cost (an estimate)

Rough monthly cost of dev in ap-south-1 at idle-to-light use; check the bill, not this table.

| Item | ~$/month |
| --- | --- |
| RDS `db.t4g.micro`, 20 GB gp3, backups | 15–20 |
| Fargate Spot: core and eve | 6–8 |
| Public IPv4: two task addresses (2 × ~$3.6) | 7 |
| Secrets Manager (~11 secrets) | 4–5 |
| KMS key | 1 |
| CloudWatch logs | 1–3 |
| **Total** | **~35–45** |

The tunnel, the WAF rate limit and the S3, EventBridge and SQS usage at dev volume add little or
nothing. More if storage grows, the class goes to `db.t4g.small`, or Spot is switched off.
