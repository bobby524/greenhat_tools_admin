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

### Option A: One-Click Deploy with Doppler (Recommended)

⚠️ **Note:** This button deploys to Vercel, but you'll configure secrets in Doppler (not Vercel).

[![Deploy with Vercel](https://vercel.com/button)](https://vercel.com/new/clone?repository-url=https%3A%2F%2Fgithub.com%2Fbobby524%2Fgreenhat_tools_admin&env=DOPPLER_TOKEN&project-name=greenhat-admin&repository-name=greenhat-admin)

**Steps:**
1. **Click the Deploy button** above
2. **Connect GitHub** and create project
3. **Set only `DOPPLER_TOKEN`** in Vercel (get from Doppler dashboard)
4. **Configure Doppler** (see Secrets Management section below)
5. **Deploy** - All other secrets come from Doppler!

### Option B: Manual Deploy with Doppler

```bash
# 1. Clone repo
git clone https://github.com/bobby524/greenhat_tools_admin.git
cd greenhat_tools_admin

# 2. Setup Doppler (see Secrets Management below)
doppler login
doppler setup --project greenhat-admin --config prd

# 3. Get Doppler token for Vercel
doppler configs tokens create vercel-token --project greenhat-admin --config prd
# Save this token!

# 4. Deploy to Vercel
npm i -g vercel
vercel login
vercel --prod

# 5. Set ONLY DOPPLER_TOKEN in Vercel
vercel env add DOPPLER_TOKEN
# Enter the token from step 3
```

### Option C: GitHub Integration + Doppler

1. Fork/clone: https://github.com/bobby524/greenhat_tools_admin
2. Go to https://vercel.com/dashboard → Add New Project
3. Import your GitHub repo
4. **Set only `DOPPLER_TOKEN`** in environment variables
5. **Connect Doppler integration** (see below)
6. Deploy

## Secrets Management with Doppler (Recommended)

**Don't store secrets in Vercel!** Use Doppler for secure secrets management.

### Why Doppler?
- ✅ Centralized secrets across all environments
- ✅ Automatic rotation
- ✅ Access logging
- ✅ Team permissions
- ✅ Version history
- ✅ Syncs to Vercel at build time

### Setup Doppler + Vercel

#### 1. Create Doppler Project

```bash
# Install Doppler CLI
brew install dopplerhq/cli/doppler

# Login
doppler login

# Create project for admin
doppler projects create greenhat-admin

# Create environments
doppler environments create prd --project greenhat-admin
```

#### 2. Add Secrets to Doppler

```bash
# Set secrets in Doppler (not in Vercel!)
doppler secrets set SUPABASE_URL "https://your-project.supabase.co" --project greenhat-admin --config prd
doppler secrets set SUPABASE_SERVICE_ROLE_KEY "your-service-role-key" --project greenhat-admin --config prd
doppler secrets set ADMIN_MCP_TOKEN "$(openssl rand -hex 32)" --project greenhat-admin --config prd
doppler secrets set ADMIN_USERNAME "admin" --project greenhat-admin --config prd
doppler secrets set ADMIN_PASSWORD "your-secure-password" --project greenhat-admin --config prd

# Verify
doppler secrets --project greenhat-admin --config prd
```

#### 3. Connect Doppler to Vercel

**Option A: Doppler Vercel Integration (Easiest)**

1. Go to [Doppler Dashboard](https://dashboard.doppler.com)
2. Select your project → Integrations
3. Click "Add Integration" → Select "Vercel"
4. Connect your Vercel account
5. Choose which Vercel project to sync
6. Map Doppler config to Vercel environment
7. Secrets auto-sync on every deploy!

**Option B: Doppler CLI in Build (More Control)**

Update `vercel.json`:
```json
{
  "buildCommand": "doppler run -- npm run build",
  "installCommand": "npm install && curl -Ls --tlsv1.2 --proto \"=https\" --retry 3 https://cli.doppler.com/install.sh | sh"
}
```

Add Doppler token to Vercel:
```bash
# Get Doppler service token
doppler configs tokens create vercel-token --project greenhat-admin --config prd

# Add to Vercel (only this one secret!)
vercel env add DOPPLER_TOKEN
```

#### 4. Update Build Scripts

```json
// package.json
{
  "scripts": {
    "build": "next build",
    "vercel-build": "doppler run -- npm run build"
  }
}
```

### Required Secrets in Doppler

| Secret | Description |
|----------|-------------|
| `SUPABASE_URL` | Your Supabase project URL |
| `SUPABASE_SERVICE_ROLE_KEY` | Service role key (NOT anon key!) |
| `ADMIN_MCP_TOKEN` | Random secret token for MCP auth |
| `ADMIN_USERNAME` | Admin login username |
| `ADMIN_PASSWORD` | Admin login password |

### Local Development with Doppler

```bash
# Clone repo
git clone https://github.com/bobby524/greenhat_tools_admin.git
cd greenhat_tools_admin

# Install dependencies
npm install

# Run with Doppler secrets
doppler run -- npm run dev

# Or configure once
doppler setup --project greenhat-admin --config prd
npm run dev  # Doppler auto-injects secrets
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

### 2. Password Protection

Simple auth with NextAuth.js or basic password:

```typescript
// app/api/auth/[...nextauth]/route.ts
import NextAuth from 'next-auth'
import CredentialsProvider from 'next-auth/providers/credentials'

const handler = NextAuth({
  providers: [
    CredentialsProvider({
      name: 'Admin',
      credentials: {
        username: { label: "Username", type: "text" },
        password: { label: "Password", type: "password" }
      },
      async authorize(credentials) {
        if (credentials?.username === process.env.ADMIN_USERNAME &&
            await verifyPassword(credentials.password, process.env.ADMIN_PASSWORD_HASH)) {
          return { id: '1', name: 'Admin', email: 'admin@greenhatsec.com' }
        }
        return null
      }
    })
  ],
  pages: {
    signIn: '/login',
  }
})

export { handler as GET, handler as POST }
```

### 3. Vercel Authentication

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
- [ ] Set up Upstash Redis (optional, for rate limiting)
- [ ] Generate `ADMIN_MCP_TOKEN` (random 64-char string)
- [ ] Generate `NEXTAUTH_SECRET` (`openssl rand -base64 32`)

### Deployment
- [ ] Click "Deploy to Vercel" button OR
- [ ] Run `vercel --prod`
- [ ] Set all environment variables
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

### MCP Tools Not Responding
- Check `/api/mcp` endpoint in browser
- Verify `ADMIN_MCP_TOKEN` matches

### Can't Access Admin
- Check Vercel password protection
- Verify IP whitelist (if enabled)
- Check middleware logs

---

**Want me to update the repo with Vercel configuration?**
