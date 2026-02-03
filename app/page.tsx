"use client";

export default function AdminDashboard() {
  return (
    <div style={{ padding: '40px', maxWidth: '1200px', margin: '0 auto' }}>
      <h1>🛡️ Greenhat Tools Admin</h1>
      <p>Platform-wide administrative interface — Secure VPN access required</p>
      
      <div style={{ 
        display: 'grid', 
        gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))',
        gap: '20px',
        marginTop: '40px'
      }}>
        <AdminCard 
          title="📊 CRM Admin"
          description="Manage customers, deals, pipelines"
          href="/admin/crm"
        />
        <AdminCard 
          title="🎫 Exponential Admin"
          description="User management, boards, security"
          href="/admin/exponential"
        />
        <AdminCard 
          title="🔒 SOC2 Admin"
          description="Compliance, audit trails"
          href="/admin/soc2"
        />
        <AdminCard 
          title="⚙️ System Admin"
          description="Global settings, backups"
          href="/admin/system"
        />
        <AdminCard 
          title="📜 Audit Logs"
          description="Platform-wide activity logs"
          href="/admin/audit"
        />
      </div>

      <div style={{ marginTop: '40px', padding: '20px', background: '#f5f5f5', borderRadius: '8px' }}>
        <h3>🔐 Security Status</h3>
        <ul>
          <li>✅ VPN Access Only</li>
          <li>✅ All Actions Logged</li>
          <li>✅ SECRET-Level Tools</li>
          <li>✅ Network Isolated</li>
        </ul>
      </div>
    </div>
  )
}

function AdminCard({ title, description, href }: { 
  title: string
  description: string
  href: string 
}) {
  return (
    <a 
      href={href}
      style={{
        display: 'block',
        padding: '24px',
        border: '1px solid #ddd',
        borderRadius: '8px',
        textDecoration: 'none',
        color: 'inherit',
        transition: 'box-shadow 0.2s',
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.boxShadow = '0 4px 12px rgba(0,0,0,0.1)'
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.boxShadow = 'none'
      }}
    >
      <h3 style={{ margin: '0 0 8px 0' }}>{title}</h3>
      <p style={{ margin: 0, color: '#666' }}>{description}</p>
    </a>
  )
}
