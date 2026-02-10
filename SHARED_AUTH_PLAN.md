# Shared Auth Plan: admin.greenhatsec.com + tools.greenhatsec.com

## ✅ Current State Confirmed

### Database Tables Already Exist
Better Auth tables are **already created** in Supabase and working:

| Table | Records | Status |
|-------|---------|--------|
| `user` | 1 | ✅ Working (Anthony's account) |
| `session` | 16 | ✅ Working (active sessions) |
| `account` | - | ✅ Created |
| `verification` | - | ✅ Created |

### User Account
- **Email:** anthony@greenhatsec.com
- **Name:** Anthony Green
- **Role:** user
- **Email Verified:** ✅

### Connection Details
- **Project:** https://hmbyzuywnpestjclchwc.supabase.co
- **Database:** postgres
- **Host:** aws-0-us-west-2.pooler.supabase.com:5432

---

## Goal
Share Better Auth sessions/cookies across `admin.greenhatsec.com` and `tools.greenhatsec.com` using the **existing** Supabase database.

---

## Configuration Requirements

### Must Match Between Both Apps

| Setting | Value | Location |
|---------|-------|----------|
| `BETTER_AUTH_SECRET` | Same secret | Vercel env vars (both apps) |
| `BETTER_AUTH_URL` | `https://tools.greenhatsec.com` | tools app |
| `BETTER_AUTH_URL` | `https://admin.greenhatsec.com` | admin app |
| Database URL | Same Supabase connection | Both apps |

### Cookie Settings (Must Match)
```typescript
advanced: {
  cookiePrefix: "greenhat_tools",
  crossSubDomainCookies: {
    enabled: true,
    domain: ".greenhatsec.com"
  },
  useSecureCookies: true
}
```

### Trusted Origins
```typescript
trustedOrigins: [
  "https://tools.greenhatsec.com",
  "https://admin.greenhatsec.com"
]
```

---

## Implementation Steps

### Step 1: Add Better Auth to Admin

Create `lib/auth.ts` in admin project:

```typescript
import { betterAuth, BetterAuthOptions } from "better-auth";
import { admin } from "better-auth/plugins";
import { Pool } from "pg";

function getDatabasePool() {
  const dbUrl = process.env.crm_POSTGRES_URL_NON_POOLING || 
                process.env.POSTGRES_URL ||
                process.env.DATABASE_URL;
  
  if (!dbUrl) throw new Error("Database URL not configured");
  
  return new Pool({
    connectionString: dbUrl,
    ssl: { rejectUnauthorized: false },
    max: 20,
    idleTimeoutMillis: 30000,
    connectionTimeoutMillis: 5000,
  });
}

export function getAuthConfig(): BetterAuthOptions {
  const pool = getDatabasePool();
  
  return {
    secret: process.env.BETTER_AUTH_SECRET!,
    baseURL: process.env.BETTER_AUTH_URL || "https://admin.greenhatsec.com",
    trustedOrigins: [
      "https://tools.greenhatsec.com",
      "https://admin.greenhatsec.com"
    ],
    database: pool,
    emailAndPassword: { enabled: true, minPasswordLength: 8 },
    socialProviders: {
      google: {
        clientId: process.env.GOOGLE_CLIENT_ID || "",
        clientSecret: process.env.GOOGLE_CLIENT_SECRET || "",
      }
    },
    plugins: [
      admin({
        adminUserIds: ["09649c79-975a-4967-9299-440b2b0fadee"],
        defaultRole: "user",
        roles: ["user", "member", "admin", "owner"]
      })
    ],
    session: { expiresIn: 60 * 60 * 24 * 7 },
    advanced: {
      useSecureCookies: process.env.NODE_ENV === "production",
      cookiePrefix: "greenhat_tools",
      crossSubDomainCookies: {
        enabled: true,
        domain: ".greenhatsec.com"
      }
    }
  };
}

// Export singleton auth
let authInstance: ReturnType<typeof betterAuth> | null = null;

export const auth = {
  handler: async (request: Request) => {
    if (!authInstance) {
      authInstance = betterAuth(getAuthConfig());
    }
    return authInstance.handler(request);
  },
  get api() {
    return authInstance?.api || {};
  }
};
```

### Step 2: Add Auth API Route

Create `app/api/auth/[...all]/route.ts`:

```typescript
import { auth } from "@/lib/auth";

export async function GET(request: Request) {
  return auth.handler(request);
}

export async function POST(request: Request) {
  return auth.handler(request);
}
```

### Step 3: Update Middleware

Update `middleware.ts` to validate sessions:

```typescript
import { NextRequest, NextResponse } from "next/server";

// API key auth for MCP
function checkApiKey(request: NextRequest): boolean {
  const authHeader = request.headers.get('authorization');
  if (!authHeader?.startsWith('Bearer ')) return false;
  return authHeader.slice(7) === process.env.ADMIN_MCP_TOKEN;
}

// Session cookie check
function getSessionCookie(request: NextRequest): string | null {
  const names = [
    "__Secure-greenhat_tools.session_token",
    "greenhat_tools.session_token"
  ];
  for (const name of names) {
    const cookie = request.cookies.get(name);
    if (cookie?.value) return cookie.value;
  }
  return null;
}

export async function middleware(request: NextRequest) {
  // MCP endpoints: API key OR session
  if (request.nextUrl.pathname.startsWith('/api/mcp')) {
    if (checkApiKey(request)) {
      return NextResponse.next();
    }
    
    // Try session auth
    const sessionToken = getSessionCookie(request);
    if (!sessionToken) {
      return NextResponse.json({ error: 'Unauthorized' }, { status: 401 });
    }
    
    // Session validation happens in the API route
    return NextResponse.next();
  }
  
  return NextResponse.next();
}

export const config = {
  matcher: ['/api/mcp/:path*']
};
```

### Step 4: Add Dependencies

```bash
npm install better-auth pg
```

### Step 5: Environment Variables

Add to Vercel (admin.greenhatsec.com):
- `BETTER_AUTH_SECRET` (same as tools)
- `BETTER_AUTH_URL` = `https://admin.greenhatsec.com`
- `crm_POSTGRES_URL_NON_POOLING` (same as tools)
- `GOOGLE_CLIENT_ID`
- `GOOGLE_CLIENT_SECRET`
- `ADMIN_MCP_TOKEN` (keep for API access)

---

## Login Flow

1. User logs in at `tools.greenhatsec.com`
2. Better Auth sets cookie with domain `.greenhatsec.com`
3. User visits `admin.greenhatsec.com`
4. Browser sends same cookie (shared domain)
5. Admin validates session against Supabase
6. User is authenticated on both sites!

---

## Testing Plan

### Test 1: Shared Session
1. Login at tools.greenhatsec.com
2. Visit admin.greenhatsec.com/api/health
3. Should show authenticated user

### Test 2: Cross-Domain Logout
1. Logout from tools.greenhatsec.com
2. Try to access admin.greenhatsec.com
3. Should be denied

### Test 3: MCP with API Key
1. Call /api/mcp with Bearer token
2. Should work without session

### Test 4: MCP with Session
1. Login on tools
2. Call /api/mcp from browser
3. Should work with session cookie

---

## Security Considerations

1. **API Key for MCP:** Keep API key auth for machine-to-machine
2. **Session for UI:** Use session auth for browser access
3. **Role Checks:** Verify admin role for sensitive endpoints
4. **CSRF:** Use SameSite=Lax for cookies (handled by Better Auth)
5. **Secure Cookies:** Enabled in production

---

## Rollback Plan

If issues occur:
1. Revert to API key only auth for MCP
2. Keep admin UI minimal
3. No shared session dependency
