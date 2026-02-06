"use client";

export default function UserAccessAdmin() {
  return (
    <div style={{ padding: '40px', maxWidth: '1200px', margin: '0 auto' }}>
      <h1>👤 User Access Management</h1>
      <p>Manage user roles, permissions, and access control</p>
      
      <div style={{ 
        display: 'grid', 
        gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))',
        gap: '20px',
        marginTop: '40px'
      }}>
        <AdminCard 
          title="👥 All Users"
          description="View and manage all platform users"
          href="/admin/users/list"
        />
        <AdminCard 
          title="🔐 Roles & Permissions"
          description="Manage roles: user, member, admin, owner"
          href="/admin/users/roles"
        />
        <AdminCard 
          title="🚫 Banned Users"
          description="View and manage banned user accounts"
          href="/admin/users/banned"
        />
        <AdminCard 
          title="📊 User Analytics"
          description="User activity and access statistics"
          href="/admin/users/analytics"
        />
      </div>

      <div style={{ marginTop: '40px', padding: '20px', background: '#fff3cd', borderRadius: '8px', border: '1px solid #ffc107' }}>
        <h3>⚠️ Security Notice</h3>
        <p>Changes to user roles and permissions take effect immediately. All actions are logged for audit purposes.</p>
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