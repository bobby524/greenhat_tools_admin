#!/bin/bash
#
# Deploy Greenhat Tools Admin to Hetzner
#
# Usage: ./scripts/deploy-hetzner.sh <hetzner-ip>
#

set -e

HETZNER_IP=${1:-}
SSH_KEY="${HOME}/.ssh/greenhat_admin"

echo "🚀 Greenhat Tools Admin - Hetzner Deployment"
echo ""

if [ -z "$HETZNER_IP" ]; then
    echo "❌ Error: Please provide Hetzner IP address"
    echo "Usage: $0 <hetzner-ip>"
    exit 1
fi

echo "📍 Target: $HETZNER_IP"
echo ""

# Check if .env exists
if [ ! -f ".env" ]; then
    echo "❌ Error: .env file not found"
    echo "Please copy .env.example to .env and configure it"
    exit 1
fi

# Generate SSH key if it doesn't exist
if [ ! -f "$SSH_KEY" ]; then
    echo "🔑 Generating SSH key..."
    ssh-keygen -t ed25519 -f "$SSH_KEY" -N ""
    echo "✅ SSH key generated: $SSH_KEY"
    echo ""
    echo "⚠️  IMPORTANT: Add this public key to your Hetzner server:"
    cat "${SSH_KEY}.pub"
    echo ""
    read -p "Press Enter when you've added the SSH key to Hetzner..."
fi

# Build containers
echo "📦 Building Docker containers..."
docker compose build

# Save images
echo "💾 Saving container images..."
docker save greenhat-tools-admin:latest > /tmp/admin-web.tar
docker save greenhat-tools-admin-mcp:latest > /tmp/admin-mcp.tar

# Create remote directory
echo "📁 Setting up remote server..."
ssh -i "$SSH_KEY" "root@$HETZNER_IP" "mkdir -p /opt/greenhat-admin"

# Copy files
echo "📤 Copying files to server..."
scp -i "$SSH_KEY" \
    /tmp/admin-web.tar \
    /tmp/admin-mcp.tar \
    docker-compose.yml \
    .env \
    "root@$HETZNER_IP:/opt/greenhat-admin/"

# Deploy on server
echo "🔧 Deploying on Hetzner..."
ssh -i "$SSH_KEY" "root@$HETZNER_IP" << EOF
    cd /opt/greenhat-admin
    
    # Load images
    echo "Loading container images..."
    docker load < admin-web.tar
    docker load < admin-mcp.tar
    
    # Stop existing if any
    docker compose down 2>/dev/null || true
    
    # Start services
    echo "Starting services..."
    docker compose up -d
    
    # Cleanup
    rm -f admin-web.tar admin-mcp.tar
    
    # Wait for services
    echo "Waiting for services to start..."
    sleep 10
    
    # Health checks
    echo "Running health checks..."
    curl -sf http://localhost:4000/api/health && echo "✅ Web app healthy" || echo "❌ Web app failed"
    curl -sf http://localhost:4002/health && echo "✅ MCP server healthy" || echo "❌ MCP server failed"
    
    echo ""
    echo "🎉 Deployment complete!"
EOF

# Cleanup local
echo "🧹 Cleaning up..."
rm -f /tmp/admin-web.tar /tmp/admin-mcp.tar

echo ""
echo "✅ Deployment finished!"
echo ""
echo "Next steps:"
echo "1. Configure WireGuard VPN client"
echo "2. Connect to VPN"
echo "3. Visit: http://10.13.13.1:4000"
echo ""
echo "WireGuard config location:"
echo "  scp -i $SSH_KEY root@$HETZNER_IP:/opt/greenhat-admin/wireguard/peer_admin1/peer_admin1.conf ./"
