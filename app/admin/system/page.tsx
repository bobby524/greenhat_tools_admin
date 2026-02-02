export default function SystemAdmin() {
  return (
    <div style={{ padding: '40px' }}>
      <h1>⚙️ System Administration</h1>
      <p>Global settings, backups, and system operations</p>
      
      <div style={{ marginTop: '40px' }}>
        <h2>Database Operations</h2>
        <ul>
          <li>Export database</li>
          <li>Run migrations</li>
          <li>Truncate tables (dangerous)</li>
        </ul>
      </div>
      
      <div style={{ marginTop: '40px' }}>
        <h2>Backups</h2>
        <ul>
          <li>Create full backup</li>
          <li>Restore from backup</li>
          <li>Manage backup storage</li>
        </ul>
      </div>
      
      <div style={{ marginTop: '40px' }}>
        <h2>System Health</h2>
        <ul>
          <li>Health check</li>
          <li>Restart services</li>
          <li>View system metrics</li>
        </ul>
      </div>
      
      <div style={{ marginTop: '40px' }}>
        <a href="/admin">← Back to Admin Dashboard</a>
      </div>
    </div>
  )
}
