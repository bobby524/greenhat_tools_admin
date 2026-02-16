# Header Hardening

The gateway must never trust caller-supplied identity headers.

## Request header stripping

The gateway strips a denylist of spoofable identity headers at the edge.
This prevents clients from attempting to inject identity/role context via headers.

Examples stripped:
- `x-user-id`, `x-org-id`, `x-roles`
- `x-forwarded-user`, `x-forwarded-email`
- `x-gotrue-claims`, `x-supabase-role`
- `proxy-authorization`

## Response security headers

The gateway sets baseline security headers on every response (unless already set):
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `Referrer-Policy: no-referrer`
- `Permissions-Policy: ...` (deny by default)
- `Cross-Origin-Resource-Policy: same-site`
- `Content-Security-Policy: default-src 'none'; frame-ancestors 'none'; base-uri 'none'`

Notes:
- HSTS is not set by default here because it depends on TLS termination.
- `X-Forwarded-For` / `X-Real-IP` are not stripped; they must still only be trusted when the reverse proxy boundary is trusted.
