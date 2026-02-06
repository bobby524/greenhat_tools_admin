"use client";

import { SignedIn, SignedOut, SignInButton, UserButton } from "@clerk/nextjs";

export default function AdminDashboard() {
  return (
    <div style={{ padding: '40px', maxWidth: '1200px', margin: '0 auto' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
        <h1>🛡️ Greenhat Tools Admin</h1>
        <SignedIn>
          <UserButton afterSignOutUrl="/" />
        </SignedIn>
      </div>
      
      <SignedOut>
        <div style={{ 
          padding: '40px', 
          background: '#f5f5f5', 
          borderRadius: '8px',
          textAlign: 'center'
        }}>
          <h2>Admin Access Required</h2>
          <p>Please sign in to access the admin dashboard.</p>
          <SignInButton mode="modal">
            <button style={{
              padding: '12px 24px',
              background: '#1a1a2e',
              color: 'white',
              border: 'none',
              borderRadius: '6px',
              cursor: 'pointer',
              fontSize: '16px',
              marginTop: '16px'
            }}>
              Sign In
            </button>
          </SignInButton>
        </div>
      </SignedOut>

      <SignedIn>
        <p>Platform-wide administrative interface</p>
        
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
          <AdminCard 
            title="🛡️ MCP Firewall"
            description="Manage tool permissions and security policies"
            href="/admin/firewall"
          />
          <AdminCard 
            title="👤 User Access"
            description="Manage user roles, permissions, and access control"
            href="/admin/users"
          />
        </div>

        <div style={{ marginTop: '40px', padding: '20px', background: '#f5f5f5', borderRadius: '8px' }}>
          <h3>🔐 Security Status</h3>
          <ul>
            <li>✅ Authentication Required</li>
            <li>✅ All Actions Logged</li>
            <li>✅ SECRET-Level Tools</li>
            <li>✅ Role-Based Access Control</li>
          </ul>
        </div>
      </SignedIn>
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
