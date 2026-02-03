# Deployment Guide

This document provides comprehensive instructions for deploying the MCP Firewall Dashboard to various environments.

## Table of Contents

- [Quick Start](#quick-start)
- [Environment Variables](#environment-variables)
- [Deployment Options](#deployment-options)
  - [Vercel (Recommended)](#vercel-recommended)
  - [Docker](#docker)
- [CI/CD Pipeline](#cicd-pipeline)
- [Pre-Deploy Checklist](#pre-deploy-checklist)
- [Troubleshooting](#troubleshooting)
- [Rollback Procedures](#rollback-procedures)

---

## Quick Start

```bash
# 1. Install dependencies
npm ci

# 2. Run tests locally
npm run test

# 3. Build locally (catches errors before deployment)
npm run build

# 4. Deploy to Vercel
vercel --prod
```

---

## Environment Variables

### Required Variables

| Variable | Description | Source |
|----------|-------------|--------|
| `NEXT_PUBLIC_SUPABASE_URL` | Supabase project URL | Supabase Dashboard |
| `SUPABASE_SERVICE_ROLE_KEY` | Supabase service role key | Supabase Dashboard → Settings → API |
| `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY` | Clerk public key | Clerk Dashboard |
| `CLERK_SECRET_KEY` | Clerk secret key | Clerk Dashboard → API Keys |

### Setting up Environment Variables

#### Local Development

Create `.env.local`:
```bash
NEXT_PUBLIC_SUPABASE_URL=https://your-project.supabase.co
SUPABASE_SERVICE_ROLE_KEY=your-service-role-key
NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=pk_test_...
CLERK_SECRET_KEY=sk_test_...
```

#### Vercel

```bash
# Using CLI
vercel env add NEXT_PUBLIC_SUPABASE_URL
vercel env add SUPABASE_SERVICE_ROLE_KEY
```

Or use Vercel Dashboard: Project Settings → Environment Variables

#### GitHub Actions Secrets

Add these secrets to your GitHub repository:
- `NEXT_PUBLIC_SUPABASE_URL`
- `SUPABASE_SERVICE_ROLE_KEY`
- `NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY`
- `CLERK_SECRET_KEY`
- `VERCEL_TOKEN`
- `VERCEL_ORG_ID`
- `VERCEL_PROJECT_ID`

---

## Deployment Options

### Vercel (Recommended)

#### Prerequisites
- Vercel CLI installed: `npm i -g vercel`
- Vercel account connected

#### Deploy Steps

```bash
# Login to Vercel (first time only)
vercel login

# Link project (first time only)
vercel link

# Deploy to preview
vercel

# Deploy to production
vercel --prod
```

#### Git Integration

When GitHub integration is enabled:
1. Every PR gets a preview deployment
2. Merges to `main` auto-deploy to production
3. See CI/CD Pipeline section for automated testing

---

### Docker

#### Build Image

```bash
# Build with build args
docker build \
  --build-arg NEXT_PUBLIC_SUPABASE_URL=https://your-project.supabase.co \
  --build-arg NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=pk_test_... \
  -t greenhat-tools-admin:latest .
```

#### Run Container

```bash
# Run with environment variables
docker run -d \
  -p 3000:3000 \
  -e SUPABASE_SERVICE_ROLE_KEY=your-key \
  -e CLERK_SECRET_KEY=your-key \
  --name greenhat-admin \
  greenhat-tools-admin:latest
```

#### Docker Compose

```yaml
version: '3.8'

services:
  app:
    build:
      context: .
      args:
        - NEXT_PUBLIC_SUPABASE_URL=${NEXT_PUBLIC_SUPABASE_URL}
        - NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY=${NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY}
    ports:
      - "3000:3000"
    environment:
      - SUPABASE_SERVICE_ROLE_KEY=${SUPABASE_SERVICE_ROLE_KEY}
      - CLERK_SECRET_KEY=${CLERK_SECRET_KEY}
    healthcheck:
      test: ["CMD", "node", "-e", "require('http').get('http://localhost:3000/api/health')"]
      interval: 30s
      timeout: 10s
      retries: 3
```

---

## CI/CD Pipeline

### GitHub Actions Workflow

The repository includes `.github/workflows/ci-cd.yml` which:

1. **Build & Test**
   - Runs on every push and PR
   - Lints and type-checks code
   - Builds the application

2. **Regression Tests**
   - Tests all API endpoints
   - Validates response formats
   - Checks response times
   - Tests CORS headers

3. **Security Scan**
   - Runs npm audit
   - Scans for vulnerabilities with Trivy

4. **Docker Build Test**
   - Builds Docker image
   - Tests image health

5. **Deploy**
   - PRs → Preview deployment
   - Main branch → Production deployment
   - Runs post-deployment smoke tests

### Pipeline Status Checks

Required checks before merging:
- ✅ Build succeeds
- ✅ All regression tests pass
- ✅ Security scan passes
- ✅ Docker build succeeds

---

## Pre-Deploy Checklist

Before deploying to production:

- [ ] Run `npm run build` locally without errors
- [ ] Run regression tests: `npm run test:regression`
- [ ] Verify all environment variables are set in Vercel
- [ ] Check that Supabase tables exist (`mcp_audit_logs`)
- [ ] Verify Clerk auth is configured correctly
- [ ] Review the PR diff for any breaking changes
- [ ] Ensure tests pass in CI/CD

---

## Troubleshooting

### Build Errors

#### "supabaseUrl is required"
**Cause**: Client initialized at module level  
**Fix**: Move client creation inside function

```typescript
// ❌ BAD
const supabase = createClient(url, key)

// ✅ GOOD
export async function GET(req) {
  const supabase = createClient(process.env.URL, process.env.KEY)
}
```

#### "Dynamic server usage"
**Cause**: Using `request.url` in edge runtime  
**Fix**: Remove `export const runtime = 'edge'` or use `export const dynamic = 'force-dynamic'`

### Runtime Errors

#### 404 on API routes
**Cause**: Clerk middleware blocking unauthenticated requests  
**Fix**: Add routes to public routes in `middleware.ts`:
```typescript
const isPublicRoute = createRouteMatcher([
  "/api/audit(.*)",
  "/api/firewall(.*)",
  // ...
])
```

#### "Cannot find module"
**Cause**: Missing dependencies or import issues  
**Fix**: 
```bash
rm -rf node_modules package-lock.json
npm install
```

### Environment Issues

#### Variables not available at build time
**Cause**: Trying to use server-side env vars during build  
**Fix**: 
- Public vars (NEXT_PUBLIC_*): Available at build and runtime
- Server vars: Only available at runtime
- Initialize clients inside functions, not at module level

---

## Rollback Procedures

### Vercel Rollback

```bash
# List recent deployments
vercel --token YOUR_TOKEN list

# Rollback to previous deployment
vercel --token YOUR_TOKEN rollback

# Rollback to specific deployment
vercel --token YOUR_TOKEN rollback DEPLOYMENT_ID
```

Or use Vercel Dashboard:
1. Go to Project → Deployments
2. Find the last working deployment
3. Click "…" → "Promote to Production"

### Docker Rollback

```bash
# Run previous image version
docker stop greenhat-admin
docker rm greenhat-admin
docker run -d \
  -p 3000:3000 \
  -e SUPABASE_SERVICE_ROLE_KEY=... \
  -e CLERK_SECRET_KEY=... \
  --name greenhat-admin \
  greenhat-tools-admin:PREVIOUS_TAG
```

### Database Rollback

If migrations failed:
```bash
# Rollback last migration
supabase db reset

# Or restore from backup
supabase db restore BACKUP_ID
```

---

## Support

For deployment issues:
1. Check CI/CD logs in GitHub Actions
2. Review Vercel deployment logs
3. Check application logs: `vercel logs`
4. Open an issue with error details
