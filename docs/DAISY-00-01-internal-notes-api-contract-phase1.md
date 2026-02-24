# Daisy Notes Internal API Contract (Phase 1)

Status: **approved for gateway scaffold (Phase 2)**  
Date: 2026-02-24  
Owner: Gateway / Daisy migration

## Scope

This contract defines the first migrated Daisy Notes internal API surface owned by the Rust gateway.

- Base path: `/api/daisy-notes`
- Internal-only: requires authenticated platform session context
- Public link sharing is explicitly disallowed

## Endpoints

### 1) List notes

`GET /api/daisy-notes/notes`

Success shape:

```json
{
  "ok": true,
  "requestId": "<string>",
  "data": {
    "notes": [
      {
        "id": "note_...",
        "title": "...",
        "content": "...",
        "tags": [],
        "pinned": false,
        "archived": false,
        "createdAt": "ISO-8601",
        "updatedAt": "ISO-8601",
        "access": {
          "role": "owner|editor|viewer",
          "canEdit": true,
          "canShare": false
        }
      }
    ],
    "scaffold": true
  },
  "error": null
}
```

### 2) Get note by id

`GET /api/daisy-notes/notes/{note_id}`

Success shape:

```json
{
  "ok": true,
  "requestId": "<string>",
  "data": {
    "note": { "...": "same note shape as list" },
    "scaffold": true
  },
  "error": null
}
```

### 3) Note share / ACL management

`GET /api/daisy-notes/notes/{note_id}/share`  
`POST /api/daisy-notes/notes/{note_id}/share`  
`DELETE /api/daisy-notes/notes/{note_id}/share`

Share object shape:

```json
{
  "id": "share_...",
  "noteId": "note_...",
  "sharedWithUserId": "user_... | null",
  "sharedWithEmail": "person@greenhatsec.com | null",
  "permission": "viewer|editor",
  "createdAt": "ISO-8601",
  "updatedAt": "ISO-8601"
}
```

## Auth context

All endpoints require an authenticated Better Auth-backed principal propagated by gateway auth middleware.

Required context:

- `principal.user_id`
- `principal.roles[]` (temporary RBAC stub input in Phase 2)
- request correlation id (`x-request-id` when available)

Missing principal returns `401` with Daisy error envelope.

## Authorization semantics (owner/editor/viewer)

Phase-1 semantic contract:

- **owner**
  - note read: allowed
  - note write (future migration phases): allowed
  - ACL/share management: allowed
- **editor**
  - note read: allowed
  - note write (future migration phases): allowed
  - ACL/share management: **forbidden**
- **viewer**
  - note read: allowed
  - note write (future migration phases): forbidden
  - ACL/share management: **forbidden**

Phase-2 gateway implementation is intentionally scaffolded and resolves role from principal roles for now (final per-note ACL resolution lands in later phases).

## Owner-only ACL management

ACL mutation endpoints (`POST/DELETE .../share`) are owner-only.

Gateway returns `403 DAISY_FORBIDDEN` when resolved role is not `owner`.

## No-public-link guarantee

Public share links are not part of internal Daisy API.

Gateway rejects payloads that request public access (`public=true`, `publicLink=true`, or `scope=public`) with:

- HTTP `400`
- error code `DAISY_PUBLIC_LINK_DISABLED`

## Error contract

All Daisy gateway endpoints use this envelope:

```json
{
  "ok": false,
  "requestId": "<string>",
  "data": null,
  "error": {
    "code": "DAISY_*",
    "message": "human-readable",
    "details": {}
  }
}
```

Initial error codes in scope:

- `DAISY_UNAUTHORIZED` (401)
- `DAISY_FORBIDDEN` (403)
- `DAISY_NOTE_NOT_FOUND` (404)
- `DAISY_INVALID_SHARE_PAYLOAD` (400)
- `DAISY_PUBLIC_LINK_DISABLED` (400)

## Migration note

This document is the Phase-1 contract input for Phase-2 gateway scaffolding only. Persistence wiring and full per-note ACL enforcement are intentionally deferred.