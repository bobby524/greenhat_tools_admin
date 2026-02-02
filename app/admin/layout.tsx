export default function AdminLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <div>
      <nav style={{
        background: '#1a1a2e',
        color: 'white',
        padding: '16px 40px',
        display: 'flex',
        gap: '24px',
        alignItems: 'center'
      }}>
        <a href="/admin" style={{ color: 'white', textDecoration: 'none', fontWeight: 'bold' }}>
          🛡️ Admin
        </a>
        <a href="/admin/crm" style={{ color: '#ccc', textDecoration: 'none' }}>CRM</a>
        <a href="/admin/exponential" style={{ color: '#ccc', textDecoration: 'none' }}>Exponential</a>
        <a href="/admin/soc2" style={{ color: '#ccc', textDecoration: 'none' }}>SOC2</a>
        <a href="/admin/system" style={{ color: '#ccc', textDecoration: 'none' }}>System</a>
        <a href="/admin/audit" style={{ color: '#ccc', textDecoration: 'none' }}>Audit</a>
      </nav>
      {children}
    </div>
  )
}
