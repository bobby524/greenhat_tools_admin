# Greenhat Tools Admin

Platform-wide administrative interface for greenhat_tools.

## Overview

Security-hardened admin panel with multiple deployment options:
- **Vercel** (Recommended) - Zero-config, auto-deploy, global CDN
- **Hetzner** - Self-hosted, VPN-only access

## Architecture (Vercel)

```
User → Vercel Edge → Admin UI / MCP Edge Functions → Supabase
```

## Quick Deploy with Vercel

[![Deploy with Vercel](https://vercel.com/button)](https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2Fbobby524%2Fgreenhat_tools_admin)

**One-click deploy to Vercel!**

### Required Secrets (in Vercel/GitHub)

| Secret | Description |
|----------|-------------|
| `SUPABASE_URL` | Your Supabase project URL |
| `SUPABASE_SERVICE_ROLE_KEY` | Service role key (NOT anon key!) |
| `ADMIN_MCP_TOKEN` | Random secret token for MCP auth |
| `ADMIN_USERNAME` | Admin login username |
| `ADMIN_PASSWORD` | Admin login password |

Set these in Vercel Dashboard → Settings → Environment Variables

## Modules

- `/admin/crm` - Customer management
- `/admin/exponential` - User & security management  
- `/admin/soc2` - Compliance & audit
- `/admin/system` - Global settings & backups
- `/admin/audit` - Platform audit logs

## Deployment Options

### Option 1: Vercel (Recommended)
See [VERCEL_DEPLOY.md](./VERCEL_DEPLOY.md) for detailed instructions.

**Pros:**
- ✅ One-click deployment
- ✅ Auto-deploys on every push
- ✅ Global CDN
- ✅ Zero server management
- ✅ Preview deployments

### Option 2: Hetzner VPS
See [HETZNER_SETUP.md](./HETZNER_SETUP.md) for VPN-only self-hosted deployment.

**Pros:**
- ✅ Complete control
- ✅ VPN-only access
- ✅ No vendor lock-in

## Security

- All tools require SECRET-level ACL
- Password-protected (Vercel) or VPN-only (Hetzner)
- All actions audited
- IP whitelisting support
- Rate limiting

## Repo

https://github.com/bobby524/greenhat_tools_admin

## Documentation

- [VERCEL_DEPLOY.md](./VERCEL_DEPLOY.md) - Vercel deployment guide
- [HETZNER_SETUP.md](./HETZNER_SETUP.md) - Hetzner self-hosted guide
