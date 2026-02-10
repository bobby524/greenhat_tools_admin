# Greenhat Admin Codebase Analysis

**Date:** February 7, 2026  
**Project:** admin.greenhatsec.com  
**Framework:** Next.js 14 + React + TypeScript  
**Location:** `/Users/bobby/Desktop/greenhat_tools_admin/`

---

## Executive Summary

This analysis identifies dead code, unused components, code duplication, performance issues, and optimization opportunities in the Greenhat Admin codebase. The codebase is a Next.js application that serves as an admin portal for the Greenhat Tools platform, including features for user management, CRM admin, MCP firewall monitoring, and invitation management.

---

## 1. Dead Code & Unused Files

### 1.1 Unused Pages (RECOMMENDED FOR DELETION)

| File | Issue | Recommendation |
|------|-------|----------------|
| `app/admin/crm/page.tsx` | Entire CRM page is unused - superseded by `/admin/greenspot` | **DELETE** - Functionality replaced by greenspot |
| `app/api/crm/contacts/route.ts` | Orphaned API route - only used by deleted crm page | **DELETE** - No consumers |
| `app/api/crm/companies/route.ts` | Orphaned API route - only used by deleted crm page | **DELETE** - No consumers |

**Evidence:**
- `/admin/crm` page imports and uses `/api/crm/contacts` and `/api/crm/companies`
- Navigation in `AdminLayout.tsx` only links to `/admin/greenspot`, not `/admin/crm`
- The greenspot page fetches from `https://tools.greenhatsec.com/api/greenspot/dashboard` (external API), not local API

### 1.2 Unused Exports & Functions

| Location | Unused Export | Used? |
|----------|--------------|-------|
| `app/admin/greenspot/data/index.ts` | `applyArchiveFilter` | No ❌ |
| `app/admin/greenspot/data/index.ts` | `applySort` | No ❌ |
| `app/admin/greenspot/data/index.ts` | `applyPagination` | No ❌ |
| `app/admin/greenspot/data/index.ts` | `toRepoError` | No ❌ |
| `app/admin/greenspot/data/index.ts` | `buildRepoResult` | No ❌ |
| `app/admin/greenspot/data/index.ts` | `buildRepoListResult` | No ❌ |
| `app/admin/greenspot/data/index.ts` | `defaultPagination` | No ❌ |
| `app/admin/greenspot/data/index.ts` | `getSupabaseClient` | Yes (indirectly) ✓ |

### 1.3 Unused Components

| Component | Location | Status |
|-----------|----------|--------|
| `Activity` icon | `app/admin/page.tsx` | Imported but unused |
| `ChevronDown` icon | `app/admin/components/AdminLayout.tsx` | Imported but unused |

### 1.4 Debug Routes (Should be removed in production)

| Route | Purpose | Risk Level |
|-------|---------|------------|
| `app/api/debug/env/route.ts` | Exposes environment variable status | **HIGH** - Info disclosure |
| `app/api/debug/session/route.ts` | Exposes session table structure | **HIGH** - Info disclosure |

**Recommendation:** Move these behind a strict admin check or remove entirely before production.

---

## 2. Code Duplication

### 2.1 Database Connection Logic (HIGH PRIORITY)

**Duplicated across 8 files:**
- `app/api/crm/contacts/route.ts`
- `app/api/crm/companies/route.ts`
- `app/api/users/route.ts`
- `app/api/invites/route.ts`
- `app/api/invites/verify/route.ts`
- `app/api/invites/accept/route.ts`
- `app/api/debug/session/route.ts`
- `app/api/migrate/route.ts`
- `lib/auth.ts` (similar pattern)

**Duplicated code block:**
```typescript
function getDatabaseUrl(): string | null {
  return (
    process.env.crm_POSTGRES_URL_NON_POOLING ||
    process.env.POSTGRES_URL ||
    process.env.DATABASE_URL ||
    process.env.CRM_POSTGRES_URL ||
    null
  );
}

function getPool() {
  const databaseUrl = getDatabaseUrl();
  if (!databaseUrl) return null;
  const isSupabase = databaseUrl.includes("supabase.co");
  return new Pool({
    connectionString: databaseUrl,
    ssl: isSupabase ? { rejectUnauthorized: false } : undefined,
    max: 5,
  });
}

// And this anti-pattern in every file!
if (process.env.NODE_TLS_REJECT_UNAUTHORIZED === undefined) {
  process.env.NODE_TLS_REJECT_UNAUTHORIZED = "0";
}
```

**Recommendation:** Create `lib/db.ts` utility module:
```typescript
// lib/db.ts
import { Pool } from "pg";

let pool: Pool | null = null;

export function getPool(): Pool | null {
  if (pool) return pool;
  // ... initialization logic
}
```

### 2.2 Session Cookie Parsing (MEDIUM PRIORITY)

**Duplicated in:**
- `app/api/invites/route.ts` - `getSessionCookie()` function
- `app/api/migrate/route.ts` - Similar cookie parsing
- `middleware.ts` - Similar cookie parsing

### 2.3 SSL Workaround (MEDIUM PRIORITY)

The `NODE_TLS_REJECT_UNAUTHORIZED = "0"` workaround is copy-pasted into **9 files**. This is a security risk and should be:
1. Moved to a single location
2. Properly documented why it's needed
3. Ideally removed and fixed properly with proper SSL certificates

---

## 3. Performance Issues

### 3.1 Supabase Client Singleton Pattern (ISSUE)

**Location:** `app/admin/greenspot/data/supabaseClient.ts`

```typescript
let supabaseClient: SupabaseClient<Database, "public"> | null = null;

export const getSupabaseClient = (): SupabaseClient<Database, "public"> => {
  if (!supabaseClient) {
    const { url, anonKey } = getSupabaseEnv();
    supabaseClient = createClient<Database, "public">(url, anonKey);
  }
  return supabaseClient;
};
```

**Issues:**
- Creates a singleton that persists across requests in serverless environment
- No request-scoped client for SSR safety
- Better Auth uses a different pattern with `Pool` which is correct

### 3.2 LocalStorage Heavy Operations (MEDIUM)

**Location:** `SettingsWorkspace.tsx`

The component makes frequent localStorage writes on every state change:
```typescript
useEffect(() => {
  if (!hasLoadedEntitySettings) return;
  saveStoredValue(getEntityStorageKey(id), { fields, sections });
  void saveEntitySettings(id, { fields, sections });
}, [fields, hasLoadedEntitySettings, id, sections]);
```

**Issues:**
- Synchronous localStorage writes block main thread
- No debouncing - every keystroke could trigger a save
- Consider using `useMemo` for derived data to reduce re-renders

### 3.3 Missing Data Fetching Optimization

**Location:** `app/admin/greenspot/page.tsx`

```typescript
async function fetchStats() {
  const response = await fetch("https://tools.greenhatsec.com/api/greenspot/dashboard");
  // No error boundary, no retry logic, no caching
}
```

**Issues:**
- No SWR/React Query for caching
- No error boundaries
- Hardcoded external URL
- No loading skeleton

### 3.4 Large Component File

**Location:** `app/admin/greenspot/SettingsWorkspace.tsx`

- **2,124 lines** - Too large for a single component
- Contains multiple components: `SectionEditor`, `DealPipelineSettings`, `EntitySettingsEditor`
- Should be split into separate files

---

## 4. Unused Dependencies

### 4.1 Potentially Unused Dependencies

| Package | Usage | Recommendation |
|---------|-------|----------------|
| `pg` | Used extensively | ✓ Keep |
| `@supabase/supabase-js` | Used in greenspot data | ✓ Keep |
| `better-auth` | Used for auth | ✓ Keep |
| `lucide-react` | Used for icons | ✓ Keep |
| `resend` | Used in email.ts | ✓ Keep |
| `zod` | **Not found in imports** | ⚠️ Verify - may be unused |

**Check for zod usage:**
```bash
grep -r "from 'zod'" --include="*.ts" --include="*.tsx" app/ lib/
```
If no results, `zod` can be removed from dependencies.

---

## 5. Security Issues

### 5.1 High Severity

| Issue | Location | Fix |
|-------|----------|-----|
| `NODE_TLS_REJECT_UNAUTHORIZED = "0"` | 9 files | Remove or make conditional on dev only |
| Debug routes expose env info | `api/debug/*` | Delete or add admin auth |
| Hardcoded admin user ID | `AdminLayout.tsx` | Move to env or database |
| `any` type usage | `AdminLayout.tsx:20` | Use proper Session type |

### 5.2 Medium Severity

| Issue | Location | Fix |
|-------|----------|-----|
| No rate limiting on invites | `api/invites/route.ts` | Add rate limiting |
| No input validation on API | Multiple routes | Add Zod validation |
| Inline styles in page.tsx | `app/page.tsx` | Move to CSS/Tailwind |

---

## 6. Architecture Improvements

### 6.1 Recommended File Structure

```
app/
├── api/
│   ├── crm/              # DELETE - unused
│   ├── invites/          # Keep - consolidate logic
│   ├── users/            # Keep
│   ├── debug/            # DELETE or secure
│   └── mcp-proxy/        # Keep
├── admin/
│   ├── components/
│   │   ├── AdminLayout.tsx
│   │   └── ui/           # New: shared UI components
│   ├── greenspot/
│   │   ├── page.tsx
│   │   ├── components/
│   │   │   ├── CrmInlineError.tsx
│   │   │   ├── SectionEditor.tsx       # Extract from SettingsWorkspace
│   │   │   ├── DealPipelineSettings.tsx # Extract from SettingsWorkspace
│   │   │   └── EntitySettingsEditor.tsx # Extract from SettingsWorkspace
│   │   └── data/
│   │       ├── index.ts
│   │       ├── types.ts
│   │       ├── customization.ts
│   │       ├── pipelineData.ts
│   │       └── supabaseClient.ts
│   ├── crm/              # DELETE - page.tsx unused
│   ├── access-controls/
│   ├── mcp-firewall/
│   ├── layout.tsx
│   └── page.tsx
├── invite/
├── lib/
│   ├── auth.ts
│   ├── auth-client.ts
│   ├── email.ts
│   └── db.ts             # NEW: shared database utility
├── types/
│   └── index.ts          # NEW: shared types
└── __tests__/            # NEW: test files
```

### 6.2 Recommended Actions Priority

| Priority | Action | Effort | Impact |
|----------|--------|--------|--------|
| P0 | Delete unused crm page and API routes | Low | High |
| P0 | Remove or secure debug endpoints | Low | High |
| P1 | Extract database utilities to lib/db.ts | Medium | High |
| P1 | Split SettingsWorkspace.tsx into components | Medium | High |
| P2 | Add input validation (Zod) to API routes | Medium | Medium |
| P2 | Add proper TypeScript types (remove `any`) | Medium | Medium |
| P3 | Remove NODE_TLS_REJECT_UNAUTHORIZED hack | High | Medium |
| P3 | Add error boundaries and loading states | Medium | Medium |

---

## 7. Testing Recommendations

### 7.1 Test Coverage Needed

| Area | Priority | Test Type |
|------|----------|-----------|
| API routes (users, invites) | High | Unit + Integration |
| Database utilities (lib/db.ts) | High | Unit |
| Data transformation functions | Medium | Unit |
| React components | Medium | Component |
| Authentication flows | High | E2E |

### 7.2 Test Files to Create

See the generated test suite in `__tests__/` directory including:
- `lib/db.test.ts`
- `api/users.test.ts`
- `api/invites.test.ts`
- `lib/email.test.ts`
- Component tests for shared UI

---

## 8. Dependencies Audit

### Current Dependencies (from package.json)

```json
{
  "@supabase/supabase-js": "^2.91.0",
  "better-auth": "^1.4.18",
  "lucide-react": "^0.460.0",
  "next": "^14.0.0",
  "pg": "^8.18.0",
  "react": "^18.2.0",
  "react-dom": "^18.2.0",
  "resend": "^6.9.1"
}
```

### Recommended Changes

| Action | Package | Reason |
|--------|---------|--------|
| Verify | `zod` | Listed in node_modules but not in package.json deps - may be sub-dep |
| Add (dev) | `@testing-library/react` | Component testing |
| Add (dev) | `@testing-library/jest-dom` | DOM assertions |
| Add (dev) | `vitest` | Test runner (already in devDeps) |
| Add (dev) | `@vitejs/plugin-react` | Vitest React support |
| Consider | `swr` or `@tanstack/react-query` | Data fetching/caching |
| Consider | `zod` | Input validation (if not already present) |

---

## Summary of Files to Delete

1. `app/admin/crm/page.tsx` - Replaced by greenspot
2. `app/api/crm/contacts/route.ts` - Orphaned
3. `app/api/crm/companies/route.ts` - Orphaned
4. `app/api/debug/env/route.ts` - Security risk
5. `app/api/debug/session/route.ts` - Security risk

---

## Summary of New Files to Create

1. `lib/db.ts` - Shared database utilities
2. `__tests__/lib/db.test.ts` - Database utility tests
3. `__tests__/api/users.test.ts` - Users API tests
4. `__tests__/api/invites.test.ts` - Invites API tests
5. `app/admin/greenspot/components/SectionEditor.tsx` - Extracted component
6. `app/admin/greenspot/components/DealPipelineSettings.tsx` - Extracted component
7. `app/admin/greenspot/components/EntitySettingsEditor.tsx` - Extracted component
8. `vitest.config.ts` - Vitest configuration

---

*Generated by OpenClaw Codebase Analysis Tool*
