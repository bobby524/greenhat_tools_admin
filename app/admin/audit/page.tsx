export default function AuditLogs() {
  return (
    <div style={{ padding: '40px' }}>
      <h1>📜 Platform Audit Logs</h1>
      <p>View all administrative actions across the platform</p>
      
      <div style={{ marginTop: '40px', padding: '20px', background: '#f5f5f5', borderRadius: '8px' }}>
        <p>Audit log viewer coming soon...</p>
        <p>Logs are stored in /app/logs/audit.log</p>
      </div>
      
      <div style={{ marginTop: '40px' }}>
        <a href="/admin">← Back to Admin Dashboard</a>
      </div>
    </div>
  )
}
