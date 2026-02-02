#!/bin/bash
#
# Setup Hetzner server for Greenhat Tools Admin
# Run this ONCE on the Hetzner server after creation
#

set -e

echo "🔧 Setting up Hetzner server for Greenhat Tools Admin"
echo ""

# Update system
echo "📦 Updating system packages..."
apt-get update
apt-get upgrade -y

# Install Docker
echo "🐳 Installing Docker..."
apt-get install -y ca-certificates curl gnupg
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/debian/gpg | gpg --dearmor -o /etc/apt/keyrings/docker.gpg
chmod a+r /etc/apt/keyrings/docker.gpg

echo "deb [arch="$(dpkg --print-architecture)" signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian "$(. /etc/os-release && echo "$VERSION_CODENAME")" stable" | tee /etc/apt/sources.list.d/docker.list > /dev/null

apt-get update
apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin

# Install Docker Compose
echo "🐳 Installing Docker Compose..."
apt-get install -y docker-compose

# Enable Docker
systemctl enable docker
systemctl start docker

# Install fail2ban for security
echo "🛡️ Installing fail2ban..."
apt-get install -y fail2ban

# Configure fail2ban
cat > /etc/fail2ban/jail.local << 'EOF'
[DEFAULT]
bantime = 3600
maxretry = 3

[sshd]
enabled = true
port = 22
filter = sshd
logpath = /var/log/auth.log
EOF

systemctl enable fail2ban
systemctl start fail2ban

# Configure firewall
echo "🧱 Configuring firewall..."
apt-get install -y ufw

# Default deny
ufw default deny incoming
ufw default allow outgoing

# Allow SSH (for management)
ufw allow 22/tcp

# Allow WireGuard
ufw allow 51820/udp

# Enable firewall
ufw --force enable

# Setup log rotation
echo "📝 Setting up log rotation..."
cat > /etc/logrotate.d/greenhat-admin << 'EOF'
/opt/greenhat-admin/logs/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 0644 root root
}
EOF

# Create admin user (optional, for non-root access)
echo "👤 Creating admin user..."
useradd -m -s /bin/bash greenhat || true
usermod -aG docker greenhat

# Setup SSH hardening
echo "🔐 Hardening SSH..."
sed -i 's/#PermitRootLogin yes/PermitRootLogin prohibit-password/' /etc/ssh/sshd_config
sed -i 's/#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config
systemctl restart sshd

echo ""
echo "✅ Server setup complete!"
echo ""
echo "Next steps:"
echo "1. Add your SSH key to /root/.ssh/authorized_keys"
echo "2. Copy your .env file to /opt/greenhat-admin/"
echo "3. Run: docker compose up -d"
echo ""
echo "Security notes:"
echo "- SSH password login: DISABLED"
echo "- Firewall: Only 22/tcp and 51820/udp allowed"
echo "- Fail2ban: Active"
echo "- Docker: Installed"
