# Hetzner Setup Instructions

## Step 1: Create Hetzner Account

1. Go to https://hetzner.com/cloud
2. Click "Sign Up"
3. Use email: anthony@greenhatsec.com
4. Verify email
5. Add credit card (Molt Card ending in 2368)

## Step 2: Create Server

1. In Hetzner Console, click "Add Server"
2. Location: Choose closest to you (e.g., "Nuremberg" or "Falkenstein")
3. OS: "Ubuntu 22.04"
4. Type: "CX21" (2 vCPU, 4GB RAM, 40GB) - €5.35/month
5. Name: `greenhat-admin`
6. Click "Create & Buy"

## Step 3: Add SSH Key

```bash
# Get the public key from me, then:
# In Hetzner Console → Project → Security → SSH Keys
# Add the key with name: "greenhat-admin-key"
```

## Step 4: Configure Firewall

In Hetzner Console:
1. Go to "Firewalls"
2. Create new firewall named "greenhat-admin"
3. Rules:
   - TCP 22 (SSH) - YOUR_IP_ONLY
   - UDP 51820 (WireGuard) - Any
4. Apply to your server

## Step 5: DNS Setup (You'll do this)

Add these DNS records for `admin.greenhatsec.com`:

```
Type: A
Name: admin
Value: [YOUR_HETZNER_IP]
TTL: 3600
```

## Step 6: Send Me the IP

Once server is created, send me the IP address. I'll deploy everything.

**Cost: ~€7/month** (Server €5.35 + Backups €1 + Volume €1)

## What Gets Deployed

- Admin web app (port 4000)
- Admin MCP server (port 4002)
- WireGuard VPN (port 51820)
- Automatic backups
- Fail2ban security
- UFW firewall

## Access After Deployment

1. Download WireGuard config from server
2. Connect to VPN
3. Visit: http://10.13.13.1:4000
4. Login with admin credentials
