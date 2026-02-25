# Deployment Target

**Target Infrastructure**: Fly.io
**App Name**: `api-mcp-gateway` (or equivalent Fly app for Rust API)
**App Type**: Rust API Backend

## Enforcement Notes
- Production deployments are strictly managed by CI/CD via GitHub Actions.
- Local deployments using `fly deploy` or `make deploy` are guarded.
- Only changes merged to the `main` branch will automatically deploy to production.
