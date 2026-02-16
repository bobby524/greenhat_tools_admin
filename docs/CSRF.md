# CSRF Protection

The gateway enforces **double-submit cookie** CSRF protection on every
state-changing request (POST / PUT / PATCH / DELETE) that flows through the
cookie-authenticated SPA session layer.

## How It Works

```
 Browser (SPA)                         Gateway
 ────────────────────────────────────────────────────
  GET /dashboard          ──────►
                          ◄──────  200 OK
                                   Set-Cookie: csrf_token=<random>; Path=/; SameSite=Lax

  (JS reads the cookie)

  POST /api/tools/invoke  ──────►
    Cookie: csrf_token=<random>      ← browser sends automatically
    X-CSRF-Token: <random>           ← SPA attaches manually
                          ◄──────  200 OK   (tokens match ✓)

  POST /api/tools/invoke  ──────►
    Cookie: csrf_token=<random>
    (no header)
                          ◄──────  403 Forbidden — CSRF token missing
```

1. On every **safe-method** (GET / HEAD / OPTIONS) response the gateway sets a
   `csrf_token` cookie with a fresh random value.
2. The cookie is intentionally **not** `HttpOnly` so the SPA JavaScript can
   read it (e.g. `document.cookie` or a cookie-parsing helper).
3. Before any **state-changing** request, the SPA copies the cookie value into
   the `X-CSRF-Token` request header.
4. The middleware compares the two values.  If they are absent, empty, or do
   not match, the request is rejected with **403 Forbidden**.

### Why This Works

An attacker on a different origin *can* trigger the browser to send the
cookie (it travels automatically), but **cannot read its value** due to the
Same-Origin Policy.  Without the value the attacker cannot set the
`X-CSRF-Token` header, so the double-submit check fails.

`SameSite=Lax` provides additional protection: the browser will not attach the
cookie to cross-site POST/PUT/PATCH/DELETE requests at all.

## Configuration

| Env var             | Default          | Description                                     |
|---------------------|------------------|-------------------------------------------------|
| `CSRF_ENABLED`      | `true`           | Master switch.  Set to `false` to disable.      |
| `CSRF_COOKIE_NAME`  | `csrf_token`     | Name of the CSRF cookie.                        |
| `CSRF_HEADER_NAME`  | `x-csrf-token`   | Name of the header the SPA must echo.           |

All three can be overridden at deploy time via environment variables or
`.env`.

## Exempt Paths

The following paths are unconditionally exempt from CSRF enforcement
(they are infrastructure probes that never carry session cookies):

- `/health`
- `/version`
- `/metrics`

## SPA Integration (BetterAuth)

BetterAuth's cookie-based session flow is fully compatible:

```ts
// Example: reading the CSRF cookie and attaching the header
function getCookie(name: string): string | undefined {
  return document.cookie
    .split("; ")
    .find((c) => c.startsWith(`${name}=`))
    ?.split("=")[1];
}

const res = await fetch("/api/tools/invoke", {
  method: "POST",
  credentials: "include",            // send session + CSRF cookies
  headers: {
    "Content-Type": "application/json",
    "X-CSRF-Token": getCookie("csrf_token") ?? "",
  },
  body: JSON.stringify({ tool: "…", input: {…} }),
});
```

## Middleware Stack Position

```
Request ─► SetRequestId ─► Trace ─► PropagateRequestId ─► RateLimit ─► Validate ─► CSRF ─► Auth ─► Handler
```

CSRF runs **after** input validation (body-size + content-type checks) and
**before** the auth layer.  This ensures malformed requests are rejected
cheaply before the CSRF check runs, while CSRF failures block the request
before any authenticated business logic executes.

## Testing

```bash
cargo test --test csrf      # run the dedicated CSRF test suite
cargo test                  # full suite including CSRF
```
