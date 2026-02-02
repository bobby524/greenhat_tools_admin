# Greenhat Tools Admin

Platform-wide administrative interface for greenhat_tools.

## Overview

Security-hardened admin panel deployed on Hetzner VPS with VPN-only access.

## Architecture

```
Internet → Hetzner VPS → WireGuard VPN → Admin Panel
```

## Access

1. Connect to WireGuard VPN
2. Visit: http://10.13.13.1:4000
3. Login with admin credentials

## Modules

- `/admin/crm` - Customer management
- `/admin/exponential` - User & security management
- `/admin/soc2` - Compliance & audit
- `/admin/system` - Global settings & backups

## Deployment

```bash
./scripts/deploy-hetzner.sh
```

## Security

- VPN-only access (WireGuard)
- All actions logged
- SECRET-level MCP tools only

## Repo

https://github.com/bobby524/greenhat_tools_admin
