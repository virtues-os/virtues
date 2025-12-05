# Atlas Control Plane

**Atlas** is the control plane for Virtues multi-tenant infrastructure. It's a SvelteKit app deployed on Vercel that manages signups, provisioning, billing, and tenant lifecycle.

---

## Overview

```text
                       [Internet]
                           │
                 [Route53: *.virtues.com]
                           │
      ┌────────────────────┼────────────────────┐
      │                    │                    │
adamjace.virtues.com   bob.virtues.com      [...]
      │                    │
      ▼                    ▼
┌─────────────┐      ┌─────────────┐
│ Hetzner VPS │      │ Hetzner VPS │
│ CPX21/CCX13 │      │ CPX21/CCX13 │
│             │      │             │
│ - Postgres  │      │ - Postgres  │
│ - Rust API  │      │ - Rust API  │
│ - SvelteKit │      │ - SvelteKit │
│ - Caddy     │      │ - Caddy     │
└─────────────┘      └─────────────┘
      │                    │
      └────────┬───────────┘
               │
      [Atlas Control Plane]
        (Vercel: virtues.com)
               │
 ┌─────────────┼─────────────┐
 │             │             │
[Hetzner] [Route53] [Stripe]
```

**Key Principle**: VPS-per-tenant. Each user gets their own isolated server with PostgreSQL, API, and encryption keys. Atlas orchestrates provisioning via API calls.

---

## Pricing & Tiers

| | **Starter ($29/mo)** | **Pro ($79/mo)** |
|--|----------------------|------------------|
| **VPS** | CPX21 (Shared AMD) | CCX13 (Dedicated AMD) |
| **vCPU** | 3 (shared) | 2 (dedicated) |
| **RAM** | 4 GB | 8 GB |
| **Storage** | 80 GB NVMe | 80 GB NVMe |
| **Hosting Cost** | ~$9.50 | ~$14 |
| **LLM Budget** | $6/mo (~4M tokens) | $20/mo (~15M tokens) |
| **Monthly Tokens** | 4,000,000 | 15,000,000 |

### Unit Economics

**Starter ($29/mo)**: $16 COGS → $13 margin (45%)
**Pro ($79/mo)**: $34.50 COGS → $44.50 margin (56%)

---

## Technology Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Framework | SvelteKit | Consistent with Virtues web app |
| Hosting | Vercel | Zero-ops, serverless |
| Database | Vercel Postgres | Managed, automatic backups |
| VPS Provider | Hetzner Cloud | Best price/performance, US East |
| DNS | AWS Route53 | Existing zone, 10k records free |
| Payments | Stripe | Subscriptions, webhooks |
| Email | Resend | Transactional emails |
| Auth | Auth.js | Magic link for admin |

---

## Database Schema (Vercel Postgres)

```sql
-- Signup requests (pre-payment)
CREATE TABLE signups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    subdomain TEXT UNIQUE NOT NULL,
    tier TEXT NOT NULL DEFAULT 'starter',
    status TEXT NOT NULL DEFAULT 'pending', -- pending, paid, provisioning, active, failed
    stripe_checkout_session_id TEXT,
    stripe_customer_id TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Active tenants
CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subdomain TEXT UNIQUE NOT NULL,
    email TEXT NOT NULL,
    tier TEXT NOT NULL DEFAULT 'starter',

    -- Hetzner
    hetzner_server_id TEXT NOT NULL,
    ip_address INET NOT NULL,

    -- Status
    status TEXT DEFAULT 'provisioning', -- provisioning, active, suspended, deleted

    -- Stripe
    stripe_customer_id TEXT NOT NULL,
    stripe_subscription_id TEXT NOT NULL,

    -- Secrets (encrypted)
    db_password_encrypted TEXT NOT NULL,
    encryption_key_encrypted TEXT NOT NULL,

    -- Timestamps
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    suspended_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

-- Provisioning audit log
CREATE TABLE provisioning_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID REFERENCES tenants(id),
    signup_id UUID REFERENCES signups(id),
    action TEXT NOT NULL, -- created, dns_added, booted, migrated, ready, failed
    details JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Admin users
CREATE TABLE admins (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_signups_status ON signups(status);
CREATE INDEX idx_signups_email ON signups(email);
CREATE INDEX idx_tenants_status ON tenants(status);
CREATE INDEX idx_tenants_email ON tenants(email);
CREATE INDEX idx_provisioning_logs_tenant ON provisioning_logs(tenant_id);
```

---

## Routes & Pages

### Public Routes

| Route | Purpose |
|-------|---------|
| `/` | Landing page with pricing |
| `/signup` | Signup form (subdomain, email, tier) |
| `/signup/checkout` | Stripe Checkout redirect |
| `/signup/success` | Post-payment confirmation |

### Admin Routes (Protected)

| Route | Purpose |
|-------|---------|
| `/admin` | Dashboard overview |
| `/admin/signups` | Pending signups list |
| `/admin/tenants` | Active tenants list |
| `/admin/tenants/[id]` | Tenant detail & actions |
| `/admin/deploy` | Manual deploy trigger |

### API Routes

| Route | Purpose |
|-------|---------|
| `POST /api/signup` | Create signup record |
| `POST /api/stripe/webhook` | Stripe webhook handler |
| `POST /api/admin/provision` | Trigger VPS provisioning |
| `POST /api/admin/suspend` | Suspend tenant |
| `POST /api/admin/unsuspend` | Reactivate tenant |
| `DELETE /api/admin/tenant/[id]` | Delete tenant (destroy VPS) |
| `GET /api/admin/tenants` | List all tenants (for deploy script) |

---

## User Flow

### Signup Flow

```
1. User visits virtues.com
2. Selects tier (Starter/Pro)
3. Enters email + desired subdomain
4. Redirected to Stripe Checkout
5. Stripe webhook: payment_intent.succeeded
6. Atlas creates signup record (status: paid)
7. Admin reviews & approves in dashboard
8. Atlas provisions VPS:
   a. Generate secrets (DB password, encryption key)
   b. Call Hetzner API to create server with cloud-init
   c. Create Route53 A record
   d. Wait for health check
   e. Send welcome email
9. User receives email with login link
10. User logs in at subdomain.virtues.com
```

### Provisioning Steps (Detailed)

```typescript
async function provisionTenant(signup: Signup): Promise<Tenant> {
  // 1. Generate secrets
  const dbPassword = crypto.randomBytes(32).toString('hex');
  const encryptionKey = crypto.randomBytes(32).toString('hex');
  const authSecret = crypto.randomBytes(32).toString('base64');

  // 2. Create Hetzner server
  const server = await hetzner.servers.create({
    name: `virtues-${signup.subdomain}`,
    server_type: signup.tier === 'pro' ? 'ccx13' : 'cpx21',
    location: 'ash', // Ashburn, VA
    image: 'debian-12',
    ssh_keys: [ATLAS_SSH_KEY_ID],
    user_data: generateCloudInit({
      subdomain: signup.subdomain,
      tier: signup.tier,
      ownerEmail: signup.email,
      dbPassword,
      encryptionKey,
      authSecret,
      resendApiKey: RESEND_API_KEY,
      ghcrRepo: GHCR_REPO,
    }),
  });

  // 3. Create DNS record
  await route53.createARecord({
    name: `${signup.subdomain}.virtues.com`,
    value: server.public_net.ipv4.ip,
    ttl: 300,
  });

  // 4. Create tenant record
  const tenant = await db.tenants.create({
    subdomain: signup.subdomain,
    email: signup.email,
    tier: signup.tier,
    hetzner_server_id: server.id.toString(),
    ip_address: server.public_net.ipv4.ip,
    stripe_customer_id: signup.stripe_customer_id,
    stripe_subscription_id: await createSubscription(signup),
    db_password_encrypted: encrypt(dbPassword),
    encryption_key_encrypted: encrypt(encryptionKey),
  });

  // 5. Wait for health check (poll every 30s, timeout 10min)
  await waitForHealth(`https://${signup.subdomain}.virtues.com/health`);

  // 6. Send welcome email
  await resend.emails.send({
    to: signup.email,
    subject: 'Welcome to Virtues',
    html: welcomeEmailTemplate(signup.subdomain),
  });

  return tenant;
}
```

---

## External API Integration

### Hetzner Cloud API

```typescript
// hetzner.ts
import { HetznerCloud } from 'hcloud-js';

const hetzner = new HetznerCloud({ token: HETZNER_API_TOKEN });

// Create server
await hetzner.servers.create({
  name: string,
  server_type: 'cpx21' | 'ccx13',
  location: 'ash',
  image: 'debian-12',
  ssh_keys: [number],
  user_data: string, // cloud-init
});

// Delete server
await hetzner.servers.delete(serverId);

// Resize server (tier upgrade)
await hetzner.servers.changeType(serverId, 'ccx13');
```

### AWS Route53

```typescript
// route53.ts
import { Route53Client, ChangeResourceRecordSetsCommand } from '@aws-sdk/client-route-53';

const route53 = new Route53Client({
  credentials: {
    accessKeyId: AWS_ACCESS_KEY_ID,
    secretAccessKey: AWS_SECRET_ACCESS_KEY,
  },
});

// Create A record
await route53.send(new ChangeResourceRecordSetsCommand({
  HostedZoneId: HOSTED_ZONE_ID,
  ChangeBatch: {
    Changes: [{
      Action: 'CREATE',
      ResourceRecordSet: {
        Name: `${subdomain}.virtues.com`,
        Type: 'A',
        TTL: 300,
        ResourceRecords: [{ Value: ipAddress }],
      },
    }],
  },
}));

// Delete A record
await route53.send(new ChangeResourceRecordSetsCommand({
  HostedZoneId: HOSTED_ZONE_ID,
  ChangeBatch: {
    Changes: [{
      Action: 'DELETE',
      ResourceRecordSet: {
        Name: `${subdomain}.virtues.com`,
        Type: 'A',
        TTL: 300,
        ResourceRecords: [{ Value: ipAddress }],
      },
    }],
  },
}));
```

### Stripe

```typescript
// stripe.ts
import Stripe from 'stripe';

const stripe = new Stripe(STRIPE_SECRET_KEY);

// Products & Prices (create once in Stripe dashboard)
const PRICES = {
  starter: 'price_xxx', // $29/mo
  pro: 'price_yyy',     // $79/mo
};

// Create checkout session
const session = await stripe.checkout.sessions.create({
  mode: 'subscription',
  line_items: [{ price: PRICES[tier], quantity: 1 }],
  customer_email: email,
  success_url: `https://virtues.com/signup/success?session_id={CHECKOUT_SESSION_ID}`,
  cancel_url: 'https://virtues.com/signup',
  metadata: { subdomain, tier },
});

// Create subscription (after checkout)
const subscription = await stripe.subscriptions.create({
  customer: customerId,
  items: [{ price: PRICES[tier] }],
});

// Webhook events to handle
// - checkout.session.completed → Mark signup as paid
// - invoice.payment_succeeded → Record payment
// - invoice.payment_failed → Flag for review
// - customer.subscription.deleted → Suspend tenant
```

---

## Cloud-Init Template

```yaml
#cloud-config
package_update: true

ssh_authorized_keys:
  - ${ATLAS_SSH_PUBLIC_KEY}

write_files:
  - path: /opt/virtues/.env
    permissions: '0600'
    content: |
      SUBDOMAIN=${subdomain}
      TIER=${tier}
      OWNER_EMAIL=${ownerEmail}
      DB_PASSWORD=${dbPassword}
      VIRTUES_ENCRYPTION_KEY=${encryptionKey}
      AUTH_SECRET=${authSecret}
      RESEND_API_KEY=${resendApiKey}
      GHCR_REPO=${ghcrRepo}
      EMAIL_FROM=Virtues <noreply@virtues.com>

runcmd:
  - curl -sSL https://raw.githubusercontent.com/yourorg/virtues/main/deploy/setup.sh | bash
```

---

## Environment Variables

```env
# Vercel Postgres
DATABASE_URL=postgres://...

# Auth
AUTH_SECRET=... # For admin auth

# Hetzner
HETZNER_API_TOKEN=...
ATLAS_SSH_KEY_ID=... # SSH key ID in Hetzner
ATLAS_SSH_PRIVATE_KEY=... # For deploy script

# AWS Route53
AWS_ACCESS_KEY_ID=...
AWS_SECRET_ACCESS_KEY=...
HOSTED_ZONE_ID=...

# Stripe
STRIPE_SECRET_KEY=...
STRIPE_WEBHOOK_SECRET=...
STRIPE_PRICE_STARTER=price_xxx
STRIPE_PRICE_PRO=price_yyy

# Resend
RESEND_API_KEY=...

# Virtues
GHCR_REPO=ghcr.io/virtues-os
GOOGLE_API_KEY=... # Passed to tenant VPS
AI_GATEWAY_API_KEY=... # Optional

# Shared Object Storage (Hetzner Object Storage)
# All tenants share one bucket with subdomain-prefixed paths
S3_ENDPOINT=https://fsn1.your-objectstorage.com
S3_BUCKET=virtues-streams
S3_ACCESS_KEY=... # Hetzner Object Storage access key
S3_SECRET_KEY=... # Hetzner Object Storage secret key
```

---

## Admin Authentication

Single admin user via magic link. `ADMIN_EMAIL` env var restricts who can log in.

```typescript
// apps/atlas/src/lib/server/auth.ts
export const { handle, signIn, signOut } = SvelteKitAuth({
  adapter: createPostgresAdapter(),
  providers: [
    {
      id: 'resend',
      type: 'email',
      async sendVerificationRequest({ identifier: email, url }) {
        if (email !== ADMIN_EMAIL) return; // Silent fail
        await resend.emails.send({
          to: email,
          subject: 'Atlas Admin Login',
          html: `<a href="${url}">Sign in to Atlas</a>`,
        });
      },
    },
  ],
  callbacks: {
    async signIn({ user }) {
      return user.email === ADMIN_EMAIL;
    },
  },
});
```

---

## Tenant Lifecycle

### Status Transitions

```
pending → paid → provisioning → active
                      ↓
                   failed

active → suspended → active (unsuspend)
           ↓
        deleted
```

### Suspend Tenant

```typescript
async function suspendTenant(tenantId: string): Promise<void> {
  const tenant = await db.tenants.findById(tenantId);

  // SSH to VPS and stop services (but keep data)
  await ssh(tenant.ip_address, 'docker compose -f /opt/virtues/docker-compose.yml stop');

  await db.tenants.update(tenantId, {
    status: 'suspended',
    suspended_at: new Date(),
  });
}
```

### Delete Tenant

```typescript
async function deleteTenant(tenantId: string): Promise<void> {
  const tenant = await db.tenants.findById(tenantId);

  // 1. Cancel Stripe subscription
  await stripe.subscriptions.cancel(tenant.stripe_subscription_id);

  // 2. Delete DNS record
  await route53.deleteARecord(`${tenant.subdomain}.virtues.com`, tenant.ip_address);

  // 3. Delete Hetzner server
  await hetzner.servers.delete(tenant.hetzner_server_id);

  // 4. Mark as deleted
  await db.tenants.update(tenantId, {
    status: 'deleted',
    deleted_at: new Date(),
  });
}
```

---

## Deploy Script

SSH-based deployment to all tenant VPS instances:

```bash
#!/bin/bash
# deploy.sh - Deploy new version to all tenants
# Usage: ./deploy.sh [TAG]

TAG=${1:-latest}

# Get all active tenant IPs from Atlas
TENANTS=$(curl -s https://virtues.com/api/admin/tenants \
  -H "Authorization: Bearer $ATLAS_TOKEN" | jq -r '.[] | select(.status == "active") | .ip_address')

deploy_one() {
  IP=$1
  echo "[$IP] Deploying tag $TAG..."
  ssh -o ConnectTimeout=10 -o StrictHostKeyChecking=no root@$IP "
    cd /opt/virtues &&
    docker compose pull &&
    docker compose down &&
    TAG=$TAG docker compose up -d &&
    sleep 5 &&
    docker compose exec -T core virtues migrate
  " 2>&1 | sed "s/^/[$IP] /"
}
export -f deploy_one
export TAG

echo "Deploying to $(echo "$TENANTS" | wc -l) tenants..."
echo "$TENANTS" | xargs -P 10 -I {} bash -c 'deploy_one "$@"' _ {}
echo "Deploy complete"
```

---

## Monitoring & Alerts

### Hetzner Built-in Monitoring

Enable via API during provisioning:
- CPU alerts (>90% for 10min)
- Memory alerts (>90%)
- Disk alerts (>85%)

### Health Check Endpoint

Atlas polls `/health` on each tenant every 5 minutes:

```typescript
// Vercel cron job: /api/cron/health
export async function GET() {
  const tenants = await db.tenants.findAll({ status: 'active' });

  for (const tenant of tenants) {
    try {
      const res = await fetch(`https://${tenant.subdomain}.virtues.com/health`, {
        signal: AbortSignal.timeout(5000),
      });
      if (!res.ok) throw new Error(`Status ${res.status}`);
    } catch (error) {
      await notifyAdmin(`Health check failed: ${tenant.subdomain}`, error);
    }
  }
}
```

---

## Implementation Plan

### Phase 1: Foundation

1. **Create SvelteKit project**
   ```bash
   npx sv create atlas --template minimal --types ts
   cd atlas
   npm install
   ```

2. **Setup Vercel Postgres**
   - Create database in Vercel dashboard
   - Run migrations

3. **Implement auth**
   - Magic link with Resend
   - Admin route protection

4. **Create admin dashboard**
   - Signups list
   - Tenants list
   - Basic CRUD

### Phase 2: Stripe Integration

1. **Create Stripe products/prices**
2. **Implement checkout flow**
3. **Setup webhook handler**
4. **Test payment flow end-to-end**

### Phase 3: Provisioning

1. **Hetzner API integration**
2. **Route53 API integration**
3. **Cloud-init generation**
4. **Health check polling**
5. **Welcome email**

### Phase 4: Tenant Management

1. **Suspend/unsuspend**
2. **Delete tenant**
3. **Tier upgrade**
4. **Deploy script**

### Phase 5: Polish

1. **Landing page**
2. **Signup form validation**
3. **Error handling**
4. **Audit logging**
5. **Admin notifications**

---

## Key Files Structure

```
atlas/
├── src/
│   ├── lib/
│   │   ├── server/
│   │   │   ├── auth.ts           # Auth.js config
│   │   │   ├── db.ts             # Database connection
│   │   │   ├── schema.ts         # Drizzle schema
│   │   │   ├── hetzner.ts        # Hetzner API client
│   │   │   ├── route53.ts        # Route53 API client
│   │   │   ├── stripe.ts         # Stripe client
│   │   │   ├── resend.ts         # Email client
│   │   │   ├── provision.ts      # Provisioning logic
│   │   │   └── cloud-init.ts     # Cloud-init template
│   │   └── components/
│   │       ├── SignupForm.svelte
│   │       ├── TenantList.svelte
│   │       └── TenantDetail.svelte
│   ├── routes/
│   │   ├── +page.svelte          # Landing page
│   │   ├── signup/
│   │   │   ├── +page.svelte      # Signup form
│   │   │   └── success/+page.svelte
│   │   ├── admin/
│   │   │   ├── +layout.server.ts # Auth guard
│   │   │   ├── +page.svelte      # Dashboard
│   │   │   ├── signups/+page.svelte
│   │   │   └── tenants/
│   │   │       ├── +page.svelte
│   │   │       └── [id]/+page.svelte
│   │   └── api/
│   │       ├── signup/+server.ts
│   │       ├── stripe/
│   │       │   └── webhook/+server.ts
│   │       └── admin/
│   │           ├── provision/+server.ts
│   │           ├── suspend/+server.ts
│   │           └── tenants/+server.ts
│   └── hooks.server.ts
├── drizzle/
│   └── migrations/
├── static/
├── package.json
├── svelte.config.js
└── vercel.json
```

---

## Security Considerations

1. **Secret Management**
   - Tenant secrets encrypted at rest in Vercel Postgres
   - Use Vercel's built-in environment variable encryption
   - Never log secrets

2. **Admin Access**
   - Single admin email restriction
   - Magic link auth (no password)
   - All admin actions logged

3. **API Security**
   - Stripe webhook signature verification
   - Rate limiting on signup endpoint
   - Input validation (subdomain format, email)

4. **VPS Security**
   - SSH key only (no password auth)
   - Firewall: only 80/443 open
   - Automatic security updates

---

## Operational Runbook

### Provision Fails

1. Check Hetzner API response in provisioning logs
2. Verify cloud-init script runs: `ssh root@IP 'cat /var/log/cloud-init-output.log'`
3. Check VPS status in Hetzner console
4. Manual retry: re-trigger provisioning from admin dashboard

### Health Check Fails

1. SSH to VPS: `ssh root@IP`
2. Check services: `docker compose ps`
3. Check logs: `docker compose logs --tail=100`
4. Restart: `docker compose restart`

### Payment Failed

1. Check Stripe dashboard for details
2. Contact customer if needed
3. If unresolved after 7 days, suspend tenant

### Tier Upgrade

1. User requests upgrade (email or in-app)
2. Admin updates Stripe subscription
3. Hetzner resize: `hetzner servers change-type ID ccx13`
4. VPS reboots (~30 seconds)
5. Update tenant tier in database

---

## Infrastructure Limits

| Service | Limit | Action at Scale |
|---------|-------|-----------------|
| Hetzner Servers | ~100 default | Request quota increase |
| Route53 Records | 10,000 free | $0.0015/record above |
| Stripe | No limit | - |
| Vercel Postgres | 60 connections | Consider PgBouncer |

---

## Next Steps

1. Create new repository: `atlas`
2. Initialize SvelteKit project
3. Setup Vercel deployment
4. Configure environment variables
5. Run database migrations
6. Implement auth + admin routes
7. Integrate Stripe
8. Implement provisioning
9. Test full signup → provision flow
10. Launch!
