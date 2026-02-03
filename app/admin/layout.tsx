import { UserButton } from "@clerk/nextjs";
import { currentUser } from "@clerk/nextjs/server";

export default async function AdminLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const user = await currentUser();
  
  return (
    <div>
      <nav style={{
        background: '#1a1a2e',
        color: 'white',
        padding: '16px 40px',
        display: 'flex',
        gap: '24px',
        alignItems: 'center',
        justifyContent: 'space-between'
      }}>
        <div style={{ display: 'flex', gap: '24px', alignItems: 'center' }}>
          <a href="/admin" style={{ color: 'white', textDecoration: 'none', fontWeight: 'bold' }}>
            🛡️ Admin
          </a>
          <a href="/admin/crm" style={{ color: '#ccc', textDecoration: 'none' }}>CRM</a>
          <a href="/admin/exponential" style={{ color: '#ccc', textDecoration: 'none' }}>Exponential</a>
          <a href="/admin/soc2" style={{ color: '#ccc', textDecoration: 'none' }}>SOC2</a>
          <a href="/admin/system" style={{ color: '#ccc', textDecoration: 'none' }}>System</a>
          <a href="/admin/audit" style={{ color: '#ccc', textDecoration: 'none' }}>Audit</a>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
          <span style={{ color: '#ccc', fontSize: '14px' }}>
            {user?.emailAddresses[0]?.emailAddress}
          </span>
          <UserButton afterSignOutUrl="/" />
        </div>
      </nav>
      {children}
    </div>
  )
}
