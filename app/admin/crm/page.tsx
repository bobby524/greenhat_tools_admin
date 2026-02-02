export default function CRMAdmin() {
  return (
    <div style={{ padding: '40px' }}>
      <h1>📊 CRM Administration</h1>
      <p>Manage all CRM data and settings</p>
      
      <div style={{ marginTop: '40px' }}>
        <h2>Quick Actions</h2>
        <ul>
          <li><a href="/admin/crm/customers">Manage Customers</a></li>
          <li><a href="/admin/crm/deals">Manage Deals</a></li>
          <li><a href="/admin/crm/pipelines">Configure Pipelines</a></li>
          <li><a href="/admin/crm/analytics">View Analytics</a></li>
        </ul>
      </div>
      
      <div style={{ marginTop: '40px', padding: '20px', background: '#fff3cd', borderRadius: '8px' }}>
        <strong>⚠️ Admin Access Required</strong>
        <p>All actions are logged and require SECRET-level MCP tools.</p>
      </div>
      
      <div style={{ marginTop: '40px' }}>
        <a href="/admin">← Back to Admin Dashboard</a>
      </div>
    </div>
  )
}
