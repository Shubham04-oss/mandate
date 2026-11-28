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
else is a 404 at Cloudflare); the MCP server (`/mcp`) and the channel proxy and registry
(`/private/v1/*`) answer only eve over the private network. Nothing listens on a public address: the core's security group admits only eve.

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
`imagetools create` stores that manifest unchanged, but it may wrap it in a new index, so the tag's
own digest can differ. The task pulls by the pinned manifest digest, so check that digest is in the
repository; if it is missing, the task fails to pull rather than run something else.

```sh
REGISTRY=$ACCOUNT.dkr.ecr.ap-south-1.amazonaws.com
DIGEST=sha256:e80acae3f1109e8e4533618b594918936f2858c63afc7204c4773148a673add8   # = db_bootstrap_image_digest
aws ecr get-login-password | docker login --username AWS --password-stdin "$REGISTRY"
docker buildx imagetools create --tag "$REGISTRY/mandate-dev/core:db-bootstrap-postgres-17.12" \
  "public.ecr.aws/docker/library/postgres@$DIGEST"
aws ecr describe-images --repository-name mandate-dev/core --image-ids imageDigest=$DIGEST   # must find it
```

Tags are immutable, so `imagetools create` fails if the tag already exists: if `describe-images`
finds the pinned digest, the mirror is already done; otherwise use a new tag. The repository's lifecycle
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

From then on every push to `main` deploys once `ci` passes on it (the workflow runs on `ci`'s
completion, not on the push). A red `main` deploys nothing; if two commits land close together and
the older one goes green last, only the newest deploys. Set it to `false` (or delete it) to stop.

## 9. First deploy

Run the workflow by hand: `gh workflow run deploy.yml --ref main`, then `gh run watch`. It builds
and pushes the images (tagged with the commit), runs the migrate task (migrations, then
`core/sql/app_role.sql`) and fails if it exits non-zero, then rolls the core and eve (eve only once
`agents/` exists; eve's old task stops before the new one starts, see Upgrading eve), following
each new ECS deployment until its rollout is `COMPLETED`; `FAILED`
(the circuit breaker rolled it back) or 25 minutes without finishing fails the job.

Each step takes the newest revision of the task definition and swaps in the new image, so after a
`tofu apply` that changes a task definition, run the workflow again to roll it out. A rerun of the
same commit skips images already pushed (tags are immutable). If the circuit breaker rolls a service
back, the job fails, naming the revision it tried.

## Migrations: expand, then contract

The migrate task runs before the services roll, and the circuit breaker (or a manual rollback) can
put the previous code back on the new schema. So every migration must work with the code before it
and after it:

1. **Expand:** add tables, columns (nullable or with a default), indexes. Deploy, along with code
   that writes both shapes and reads the new one.
2. **Contract:** once no running code uses the old shape, drop or rename it in a later PR and deploy.

Never drop, rename or change the type of something the live code uses in the same deploy as the code
change. The `migrations` workflow fails a PR whose new or edited migration contains `DROP`,
`RENAME` or `ALTER ... TYPE` (`scripts/lint-migrations.sh`); add the label
`migration-destructive-ok` only for a contract step whose expand step is already live.

## 10. The web app

The `dev` environment in `web/wrangler.jsonc` serves the app on `app.oryxa.in`. From the repository
root:

```sh
cd web && npm ci && CLOUDFLARE_ENV=dev VITE_MANDATE_API_URL=https://api.oryxa.in npm run build && npx wrangler deploy --env dev
```

`npm run deploy` in `web/` runs the same build and deploy. After the first deploy, the `web` job in
`.github/workflows/deploy.yml` runs it on every merge to main, after the core rolls out. It needs the `dev`
environment's secret `CLOUDFLARE_API_TOKEN` (a token with only Account → Workers Scripts edit and Zone `oryxa.in` → Workers Routes edit) and variable
`CLOUDFLARE_ACCOUNT_ID`. Wrangler creates the `app.oryxa.in`
custom domain; the Worker has no `workers.dev` address or preview URLs. Both values are fixed when the app is built:

- `CLOUDFLARE_ENV=dev` picks the environment. Cloudflare's Vite plugin applies it at build time;
  without it, `--env dev` on `wrangler deploy` is ignored and the top-level Worker (`mandate-web`,
  no route) is deployed. With it, `--env dev` is a check: Wrangler refuses an `--env` that doesn't
  match the build.
- `VITE_MANDATE_API_URL` is compiled into the browser code and the page's Content-Security-Policy.
  A build without it fails on purpose, so a deployed app never shows the made-up stand-in data
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

- **Code:** revert the commit on `main`; the deploy follows once `ci` passes. In a hurry, point the
  service at the previous revision:
  `aws ecs update-service --cluster mandate-dev --service core --task-definition mandate-dev-core:<previous>`
  (`aws ecs list-task-definitions --family-prefix mandate-dev-core --sort DESC`). Same for `eve`.
  That is safe only because migrations are expand-only (above): the old code runs on the new
  schema. The next deploy starts from the newest revision again, so revert on `main` as well.
- **Migrations** only go forward; the migrate task never runs a down migration. A bad migration is
  fixed by a new one, then a deploy. Rolling code back does not roll the schema back.
- **Stop deploying:** `gh variable set MANDATE_DEPLOY_ENABLED --body false`.
- **eve:** rolling eve back to an earlier eve release strands the sessions the newer one started,
  just as an upgrade does (below).

## Upgrading eve

eve's Postgres world serves one eve version at a time, so the eve service stops its task before
starting the new one (`deployment_maximum_percent = 100` in `ecs.tf`): every eve deploy has a
minute or so with no eve, and the core answers conversation requests with an error meanwhile.
The new task runs the world's `bootstrap` (its migrations, additive) before `eve start`, so a bump
of `@workflow/world-postgres` needs no separate step.

**Upgrading eve ends open sessions.** Sessions started by the previous eve release are stranded:
their history stays readable, but they cannot resume. A web conversation open across the upgrade
says it has ended (eve answers `409 session_stranded`), and an approval still waiting in one can
no longer resume the agent. Start a new conversation. Deploys that keep the eve release resume
open sessions.
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

## Telemetry (off until you approve it)

`telemetry_enabled` (default `false`) runs one OpenTelemetry Collector (`collector` service,
0.25 vCPU / 512 MB on Spot, about $10/month with its public IP and logs) and points the core and
eve at it (`OTEL_EXPORTER_OTLP_ENDPOINT=http://collector.mandate-dev.internal:4318`). It is the only
path telemetry takes to PostHog EU: it admits the core and eve only, keeps allowlisted attribute keys
(ids, routes, codes, error types), masks emails, tokens, keys, long hex/base64, 6-digit codes and IPs,
drops exception messages, stack traces and span status text, and forwards logs at WARN and above.
CloudWatch still gets every log line. The config is `infra/envs/dev/collector/config.yaml.tftpl`; the
allowlist and patterns are in `ecs.tf`, and the tests check both.

Without the switch, a normal apply still creates the empty `mandate-dev/posthog-key` secret and the
`posthog` event queue (the core's product-events consumer runs only once it has the key). To turn
telemetry on, after you have approved the cost:

```sh
put posthog-key "$POSTHOG_PROJECT_KEY"   # the EU project's token (phc_...), not a personal key (phx_...)
gh variable set POSTHOG_PROJECT_KEY --env dev --body "$POSTHOG_PROJECT_KEY"   # the web build's (public key)
# dev.tfvars: telemetry_enabled = true
tofu plan -var-file=dev.tfvars -out=dev.tfplan   # read it: collector service, role, group, rules
tofu apply dev.tfplan
gh workflow run deploy.yml --ref main            # rolls the core and eve onto the new env
aws logs tail /mandate-dev/collector --follow    # no export errors
```

The `posthog` event queue allows 50 receives (about 50 minutes of a PostHog outage) before an event
moves to its dead-letter queue. After an outage, send them back with
`aws sqs start-message-move-task --source-arn <mandate-dev-posthog-dlq.fifo ARN>`; PostHog drops
repeats by event uuid.

The core and eve also get `POSTHOG_API_KEY` and `POSTHOG_HOST`: the core for product events and
errors, eve for errors and model calls (`$ai_generation`; PostHog prices them from model and
tokens). eve's ECS health probe (`/eve/v1/health`) is not traced.

**Browser source maps.** The `web` deploy job uploads them to PostHog with `@posthog/cli` and
deletes them before publishing. It needs, in the `dev` environment, the secret
`POSTHOG_CLI_API_KEY` (a personal API key with *error tracking write* and *organization read*) and
the variable `POSTHOG_PROJECT_ID` (`367889`); without them the deploy skips the upload and still
deletes the maps.

**Checking error tracking.** Set `debug_routes = true`, apply (`aws_ecs_task_definition.core` and
`.eve`), and run the deploy workflow. Signed in as an admin, from the browser console on the app:

```js
fetch("https://api.oryxa.in/v1/debug/error", { method: "POST", credentials: "include",
  headers: { "X-Mandate": "1", "Content-Type": "application/json" }, body: '{"service":"core"}' })
// then {"service":"eve"}; and window.__mandateTestError() for the browser
```

Each shows up in PostHog's Error tracking with a `trace_id` and no message text. Set it back to
`false` and apply when done. To turn it off, set the
variable to `false`, apply, and run the deploy workflow again. `telemetry_sample_percent` (default
100) keeps every error trace and that percentage of the rest; below 100 the collector must stay a
single task. The image is from Docker Hub and pinned by digest in `ecs.tf`; to upgrade, change the tag
comment and the digest together. The image has no shell, so the task has no container health check:
ECS replaces it if the collector exits.

## Email (off until you turn it on)

`email_enabled` (default `false`) lets the core send its own email through SES Mumbai: the `email`
event queue and its dead-letter queue, the `mandate-dev` configuration set (TLS required), a rule on
the default EventBridge bus that forwards SES bounces and complaints for that set to the
`mandate-dev-email-feedback` queue (only that rule may write to it), and the core's permission to
`ses:SendEmail` as `oryxa.in` through that set only. Today it sends one email: "approval waiting",
to everyone who may decide a new approval.

Before you turn it on: the `oryxa.in` identity must be verified in SES ap-south-1 with DKIM and the
`mail.oryxa.in` MAIL FROM (done outside tofu), the account out of the SES sandbox, and the email
pictures served at `https://app.oryxa.in/email/` (the web app's deploy). SES charges about $0.10 per
1,000 emails.

```sh
# dev.tfvars: email_enabled = true
tofu plan -var-file=dev.tfvars -out=dev.tfplan   # read it: configuration set, rule, queues, IAM
tofu apply dev.tfplan
gh workflow run deploy.yml --ref main            # rolls the core onto the new env
aws logs tail /mandate-dev/core --follow         # "MANDATE_EMAIL_CONFIGURATION_SET not set" is gone
```

The image carries the templates (`emails/dist`, built in the core's Dockerfile) at `/app/emails`;
the core refuses to start if a template needs a value it doesn't fill.

To check it live, ask for an approval and read the email that arrives: from `oryxa.in`, DKIM and
DMARC passing, not in spam. For a bounce, put `bounce@simulator.amazonses.com` on a test member and
ask again: the address lands in `email_suppressions` and the outbox gets `email.bounced.v1`.

**Retries and redrive.** The `email` queue allows 20 receives (about 20 minutes of an SES or WorkOS
outage) before an event moves to `mandate-dev-email-dlq.fifo`, which alarms. Each email is claimed
in `email_sends` before it is sent, so after an outage you can send the events back with
`aws sqs start-message-move-task --source-arn <mandate-dev-email-dlq.fifo ARN>` without anyone
getting the same email twice. A bounce or complaint that can't be applied moves to
`mandate-dev-email-feedback-dlq` (also alarmed) after 5 receives; redrive it the same way.

**Suppressions.** A permanent bounce or a complaint suppresses the address for every organization
(`email_suppressions`, keyed by SHA-256 of the lowercased address). To lift one, delete its row as the
migration role once the person confirms the address works.

To turn email off, set the variable to `false`, apply, and run the deploy workflow again; events
already queued for `email` are dropped with the queue.

## Channels (off until you turn it on)

Slack, then WhatsApp and SMS through Twilio ([channels.md](channels.md)). The core sits in front of
every channel (provider webhooks arrive at `api.<domain>/v1/channels/...` through the tunnel) and
behind it (eve's provider API calls go through the core's proxy at `/private/v1/...`, which the
tunnel never forwards: eve reaches it at `http://core.mandate-dev.internal:8080`). eve never holds
a provider secret.

`channels_enabled` (default `false`) turns on:

- the `approval-notify`, `card-update` and `channel-cleanup` event queues (FIFO, 20 receives, each
  with a dead-letter queue and both alarms) and the core's `MANDATE_QUEUE_*` for them;
- on the core: `MANDATE_SLACK_CLIENT_SECRET`, `MANDATE_SLACK_SIGNING_SECRET`, `MANDATE_DECISION_KEY`,
  `MANDATE_CHANNEL_PROXY_KEY` (secrets) and `MANDATE_SLACK_CLIENT_ID`,
  `MANDATE_CREDENTIAL_PREFIX=mandate-dev/channels/`, `MANDATE_CREDENTIAL_KMS_KEY`;
- on eve: `MANDATE_CHANNEL_PROXY_KEY` and `MANDATE_CORE_PRIVATE_URL`;
- the core's task role may create, write, read and delete secrets under `mandate-dev/channels/`
  only, under the environment's KMS key only: that is where it keeps each workspace's bot token,
  created at install and deleted by `channel-cleanup` (never in tofu).

`agent-resume` also takes `approval.expired.v1` (it answers eve "deny"), and `posthog` the new event
types, whether channels are on or not. Twilio is wired only once `twilio_account_sid` is set too
(below), so Slack can go live first.

A normal apply already creates the six empty containers (`core/slack-client-secret`,
`core/slack-signing-secret`, `core/decision-key`, `core/twilio-auth-token`, `channel-proxy-key`,
`twilio-resign-key`); no task reads them until the switch is on. Put the values before you turn it
on: a task does not start while a secret it reads is empty. It is safe to turn on before the
channel code is deployed (the extra env is ignored until then).

**1. The Slack app.** At api.slack.com/apps, *Create New App*, *From a manifest*, in the workspace
you will test with (replace `<domain>`). The scopes follow the methods the proxy allows
([channels.md](channels.md) 6.3); trim them once spike 6 confirms what eve 1.5 calls.

```yaml
display_information:
  name: Mandate
features:
  app_home:
    messages_tab_enabled: true
    messages_tab_read_only_enabled: false
  bot_user:
    display_name: Mandate
    always_online: true
oauth_config:
  redirect_urls:
    - https://api.<domain>/v1/channels/slack/install/callback
    - https://api.<domain>/v1/channels/slack/link/callback
  scopes:
    bot: [app_mentions:read, assistant:write, channels:history, channels:read, chat:write,
          files:read, files:write, groups:history, groups:read, im:history, im:read, im:write,
          mpim:history, mpim:read, reactions:write]
    user: [openid]   # Sign in with Slack, to link a member: team and user id only, no email
settings:
  event_subscriptions:
    request_url: https://api.<domain>/v1/channels/slack/events
    bot_events: [app_mention, app_uninstalled, message.channels, message.groups, message.im,
                 message.mpim, tokens_revoked]
  interactivity:
    is_enabled: true
    request_url: https://api.<domain>/v1/channels/slack/interactions
  org_deploy_enabled: false
  socket_mode_enabled: false
  token_rotation_enabled: false
```

Slack checks the event URL when you save it; that passes only once the core with the gateway is
deployed and the switch is on, so save the manifest again then. Under *Manage Distribution*, turn
on public distribution only when another organization's workspace needs to install it.

**2. Secrets.** From *Basic Information*, App Credentials:

```sh
put core/slack-client-secret  "$SLACK_CLIENT_SECRET"
put core/slack-signing-secret "$SLACK_SIGNING_SECRET"
put core/decision-key         "$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')"   # 32 random bytes, base64url
put channel-proxy-key         "$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')"
```

Rotating the decision key voids the buttons on cards already sent (deciders use the web inbox); a
new proxy key needs the core and eve rolled together (run the deploy workflow).

**3. Turn it on.**

```sh
# dev.tfvars: channels_enabled = true
#             slack_client_id  = "1234567890.1234567890"   # Basic Information, Client ID (not secret)
tofu plan -var-file=dev.tfvars -out=dev.tfplan   # read it: three queues, three DLQs, six alarms, the channels policy, both task definitions
tofu apply dev.tfplan
gh workflow run deploy.yml --ref main            # rolls the core and eve onto the new env and secrets
```

**4. WhatsApp and SMS (Twilio, once stream E lands).** In the Twilio console: buy one number for the
environment (shared by every org), register it as a WhatsApp sender (needs the Meta business
verification Twilio walks you through), and create an approval card template in the Content Template
Builder (quick-reply buttons *Approve* and *Deny*) and submit it for WhatsApp approval. Set the
number's and the WhatsApp sender's incoming-message webhook to
`https://api.<domain>/v1/channels/twilio/messages` (HTTP POST): the core checks Twilio's signature
over exactly that URL (`MANDATE_API_ORIGIN` plus the path).

```sh
put core/twilio-auth-token "$TWILIO_AUTH_TOKEN"   # Account Info; eve never sees it
put twilio-resign-key      "$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')"
# dev.tfvars: twilio_account_sid       = "AC..."
#             whatsapp_from            = "whatsapp:+1555..."
#             twilio_approval_template = "HX..."   # the approved template's content SID
tofu plan -var-file=dev.tfvars -out=dev.tfplan && tofu apply dev.tfplan
gh workflow run deploy.yml --ref main
```

**5. Check it live.**

```sh
curl -s -o /dev/null -w '%{http_code}\n' -X POST https://api.<domain>/v1/channels/slack/events    # 401: unsigned
curl -s -o /dev/null -w '%{http_code}\n' -X POST https://api.<domain>/private/v1/sessions/report  # 404: never through the tunnel
aws logs tail /mandate-dev/core --follow
```

Then, in the app: as an admin, install Slack from Settings, Channels (the workspace's token lands in
`aws secretsmanager list-secrets --filters Key=name,Values=mandate-dev/channels/`); as a member,
link your Slack account; mention `@Mandate` in a channel and reply in its thread; ask for something
that needs an approval and approve it from the DM card: the card in the thread changes to
"Approved by ... on Slack" and the agent carries on. Let one expire: its cards say "Expired" and
the agent is told no. Uninstall the app from the workspace: the installation turns off and its
secret is deleted. Every `*-dlq-not-empty` and `*-consumer-stuck` alarm stays OK. For Twilio, link a
phone from Settings (a code arrives by SMS), message the number, and approve from WhatsApp.

**Retries and redrive.** The three channel queues allow 20 receives (about 20 minutes of a Slack or
Twilio outage). Each card is claimed in `channel_messages` before it is sent, so after an outage
`aws sqs start-message-move-task --source-arn <mandate-dev-approval-notify-dlq.fifo ARN>` (or
`card-update`, `channel-cleanup`) sends only what was never sent.

**Turning it off.** Set the variable to `false`, apply, and run the deploy workflow; events queued
for the three consumers are dropped with the queues. Workspace tokens under `mandate-dev/channels/`
stay: remove the installations from Settings first (while it is still on) so `channel-cleanup`
revokes and deletes them, or delete them by hand.

**Cost.** The six containers cost about $2.40/month from the first apply, on or off. On: about
$0.60 for the six alarms and $0.40 per installed workspace (its token secret); SQS and EventBridge at
dev volume are cents. Slack is free. Twilio: about $1–2/month for the number, plus per message
(SMS a cent or so in the US; WhatsApp Twilio's fee plus Meta's per-conversation price for templates,
a few cents) and the Meta verification. Roughly $3–5/month before Twilio, $5–10 with it at dev volume.

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
| Secrets Manager (~17 secrets, six of them for channels) | 7 |
| KMS key | 1 |
| CloudWatch logs | 1–3 |
| **Total** | **~37–47** |

The tunnel, the WAF rate limit and the S3, EventBridge and SQS usage at dev volume add little or
nothing. More if storage grows, the class goes to `db.t4g.small`, or Spot is switched off.
