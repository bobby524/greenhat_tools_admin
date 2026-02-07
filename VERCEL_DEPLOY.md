# Vercel Deployment Guide

Deploy Greenhat Tools Admin to Vercel with one-click deployment and zero configuration management.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Vercel Platform                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────────┐        ┌─────────────────────┐         │
│  │  Admin Web UI       │        │  MCP Edge Functions │         │
│  │  (Next.js App)      │        │  (Vercel Edge)      │         │
│  │                     │        │                     │         │
│  │  • /admin/* pages   │        │  • /api/mcp/*       │         │
│  │  • Static assets    │        │  • Serverless MCP   │         │
│  │  • Edge cached      │        │  • Rate limiting    │         │
│  │                     │        │  • Audit logs →     │         │
│  └──────────┬──────────┘        └──────────┬──────────┘         │
│             │                              │                     │
│             │                              ▼                     │
│             │                    ┌──────────────────┐            │
│             │                    │  Upstash Redis   │            │
│             │                    │  (Rate limits)   │            │
│             │                    └──────────────────┘            │
│             │                                                   │
│             └──────────────────┬────────────────┐               │
│                                │                │               │
│                                ▼                ▼               │
│                       ┌─────────────────────────────────┐       │
│                       │      Supabase Database          │       │
│                       │  • All tables (RLS protected)   │       │
│                       │  • Service role access          │       │
│                       └─────────────────────────────────┘       │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Why Vercel?

✅ **Zero Server Management** - No SSH, no updates, no maintenance  
✅ **Global CDN** - Fast access worldwide  
✅ **Automatic Deploys** - Push to GitHub → Auto-deploy  
✅ **Preview Deployments** - Every PR gets a preview URL  
✅ **Edge Functions** - MCP server runs at the edge  
✅ **Built-in Security** - DDoS protection, HTTPS, WAF  

## Deployment Options

### Option A: One-Click Deploy (Recommended)

[![Deploy with Vercel](https://vercel.com/button)](https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2Fbobby524%2Fgreenhat_tools_admin)

**Steps:**
1. **Click the Deploy button** above
2. **Connect GitHub** and create project
3. **Set environment variables** in Vercel (see below)
4. **Deploy**

### Option B: Manual Deploy

```bash
# 1. Clone repo
git clone https://github.com/bobby524/greenhat_tools_admin.git
cd greenhat_tools_admin

# 2. Deploy to Vercel
npm i -g vercel
vercel login
vercel --prod

# 3. Set environment variables in Vercel Dashboard
```

### Option C: GitHub Integration

1. Fork/clone: https://github.com/bobby524/greenhat_tools_admin
2. Go to https://vercel.com/dashboard → Add New Project
3. Import your GitHub repo
4. **Set environment variables** (see below)
5. Deploy

## Secrets Management

Set secrets directly in **Vercel Dashboard** → Settings → Environment Variables

### Required Environment Variables

| Secret | Description |
|----------|-------------|
| `SUPABASE_URL` | Your Supabase project URL |
| `SUPABASE_SERVICE_ROLE_KEY` | Service role key (NOT anon key!) |
| `ADMIN_MCP_TOKEN` | Random secret token for MCP auth |
| `BETTER_AUTH_SECRET` | Secret for auth sessions (`openssl rand -base64 32`) |
| `BETTER_AUTH_URL` | Your admin URL (e.g., `https://admin.greenhatsec.com`) |
| `GOOGLE_CLIENT_ID` | Google OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret |
| `GOOGLE_ALLOWED_DOMAIN` | Allowed Google domain (e.g., `greenhatsec.com`) |

### Optional Environment Variables

| Secret | Description |
|----------|-------------|
| `ALLOWED_IPS` | Comma-separated list of allowed IPs |
| `NEXT_PUBLIC_API_URL` | API URL for frontend |
| `NEXT_PUBLIC_SUPABASE_URL` | Supabase URL for frontend |

### Local Development

Create a `.env.local` file:

```bash
# Supabase
SUPABASE_URL=https://your-project.supabase.co
SUPABASE_SERVICE_ROLE_KEY=your-service-role-key

# Auth
BETTER_AUTH_SECRET=your-secret
BETTER_AUTH_URL=http://localhost:4000

# Google OAuth
GOOGLE_CLIENT_ID=your-client-id
GOOGLE_CLIENT_SECRET=your-client-secret
GOOGLE_ALLOWED_DOMAIN=greenhatsec.com

# MCP
ADMIN_MCP_TOKEN=random-token
```

Then run:
```bash
npm install
npm run dev
```

## Security Configuration

### 1. IP Whitelisting (Recommended)

Add IP whitelist middleware:

```typescript
// middleware.ts
import { NextResponse } from 'next/server'
import type { NextRequest } from 'next/server'

const ALLOWED_IPS = process.env.ALLOWED_IPS?.split(',') || []

export function middleware(request: NextRequest) {
  const ip = request.ip || request.headers.get('x-forwarded-for')?.split(',')[0]
  
  if (ALLOWED_IPS.length > 0 && !ALLOWED_IPS.includes(ip)) {
    return new NextResponse('Access denied', { status: 403 })
  }
  
  return NextResponse.next()
}

export const config = {
  matcher: '/admin/:path*'
}
```

Set `ALLOWED_IPS` env var with your office IP(s).

### 2. Vercel Authentication

Enable Vercel's built-in protection:

```bash
# Protect deployments with Vercel auth
vercel protection enable
```

Only team members with Vercel access can view deployments.

## Repository Changes for Vercel

### 1. Add `vercel.json`

```json
{
  "version": 2,
  "buildCommand": "npm run build",
  "installCommand": "npm install",
  "framework": "nextjs",
  "rewrites": [
    {
      "source": "/api/mcp/:path*",
      "destination": "/api/mcp"
    }
  ],
  "headers": [
    {
      "source": "/admin/:path*",
      "headers": [
        {
          "key": "X-Frame-Options",
          "value": "DENY"
        },
        {
          "key": "X-Content-Type-Options",
          "value": "nosniff"
        }
      ]
    }
  ]
}
```

### 2. Update MCP Server for Edge

Create serverless MCP endpoint:

```typescript
// app/api/mcp/route.ts
import { NextRequest, NextResponse } from 'next/server'

export const runtime = 'edge'

export async function POST(request: NextRequest) {
  // MCP logic here - runs at the edge
  const body = await request.json()
  
  // Verify auth token
  const authHeader = request.headers.get('authorization')
  if (!authHeader?.startsWith('Bearer ') || 
      authHeader.slice(7) !== process.env.ADMIN_MCP_TOKEN) {
    return NextResponse.json({ error: 'Unauthorized' }, { status: 401 })
  }
  
  // Handle tool calls
  const { name, arguments: args } = body.params
  
  // Execute admin tools...
  
  return NextResponse.json({ result: 'success' })
}
```

### 3. Add `package.json` Scripts

```json
{
  "scripts": {
    "dev": "next dev -p 4000",
    "build": "next build",
    "start": "next start",
    "vercel-build": "next build"
  }
}
```

## Domain Setup

### 1. Add Custom Domain

1. Go to Vercel Dashboard → Project → Settings → Domains
2. Add: `admin.greenhatsec.com`
3. Vercel provides DNS records:
   ```
   Type: A
   Name: admin
   Value: 76.76.21.21
   ```
4. Add DNS record in your domain provider
5. Wait for SSL certificate (automatic)

### 2. Password Protection on Vercel

```bash
# Enable Vercel Password Protection
vercel protection enable password
```

Or in dashboard: Settings → Deployment Protection → Password Protection

## Complete Deployment Checklist

### Pre-Deployment
- [ ] Fork/clone the repo
- [ ] Update `vercel.json` with your config
- [ ] Add MCP edge function
- [ ] Add Upstash Redis (optional, for rate limiting)
- [ ] Generate `ADMIN_MCP_TOKEN` (random 64-char string)
- [ ] Generate `BETTER_AUTH_SECRET` (`openssl rand -base64 32`)

### Deployment
- [ ] Click "Deploy to Vercel" button OR
- [ ] Run `vercel --prod`
- [ ] Set all environment variables in Vercel
- [ ] Wait for build to complete

### Post-Deployment
- [ ] Test admin login
- [ ] Verify MCP tools work
- [ ] Check audit logging
- [ ] Set up custom domain
- [ ] Configure IP whitelist (optional)
- [ ] Enable Vercel password protection
- [ ] Add team members to Vercel project

## Monitoring

### Vercel Analytics
Built-in analytics show:
- Traffic
- Performance
- Errors
- Usage

### Custom Monitoring

Add to `middleware.ts`:
```typescript
// Log all admin requests
console.log(`[ADMIN] ${new Date().toISOString()} | ${ip} | ${request.url}`)
```

View logs in Vercel Dashboard → Logs

## Updating

```bash
# Make changes locally
git add .
git commit -m "Update admin feature"
git push

# Vercel auto-deploys!
```

Or use Vercel's GitHub integration - every push auto-deploys.

## Cost

| Service | Cost |
|---------|------|
| Vercel Pro (for password protection) | $20/month |
| Upstash Redis (optional) | Free tier |
| Supabase | Free tier (or $25/month) |
| **Total** | **$20-45/month** |

## Security Checklist

- [ ] Environment variables set (never commit to repo)
- [ ] `ADMIN_MCP_TOKEN` is strong/random
- [ ] IP whitelist configured (optional)
- [ ] Password protection enabled
- [ ] Custom domain with SSL
- [ ] Team access limited
- [ ] Audit logging active
- [ ] Rate limiting enabled

## Troubleshooting

### Build Fails
```bash
# Check build locally
npm run build
```

### Environment Variables Not Working
- Check Vercel Dashboard → Settings → Environment Variables
- Redeploy after adding variables
- Ensure variables are set for the correct environment (Production/Preview)

### MCP Tools Not Responding
- Check `/api/mcp` endpoint in browser
- Verify `ADMIN_MCP_TOKEN` matches

### Can't Access Admin
- Check Vercel password protection
- Verify IP whitelist (if enabled)
- Check middleware logs

---

**Want me to update the repo with Vercel configuration?**
