import AdminLayout from "./components/AdminLayout";

export default function AdminDashboard() {
  return (
    <AdminLayout title="Dashboard">
      <div style={{ display: "grid", gap: "20px", gridTemplateColumns: "repeat(auto-fit, minmax(300px, 1fr))" }}>
        {/* MCP Firewall Card */}
        <DashboardCard
          title="MCP Firewall"
          description="Monitor and control agent data access"
          icon="🛡️"
          href="/admin/mcp-firewall"
          status="active"
        />

        {/* CRM Admin Card */}
        <DashboardCard
          title="CRM Admin"
          description="Manage CRM settings and configuration"
          icon="👥"
          href="/admin/crm"
          status="coming-soon"
        />

        {/* Access Controls Card */}
        <DashboardCard
          title="Access Controls"
          description="Manage user roles and permissions"
          icon="🔐"
          href="/admin/access-controls"
          status="active"
        />
      </div>

      <div style={{ marginTop: "30px", padding: "20px", background: "white", borderRadius: "8px" }}>
        <h3 style={{ margin: "0 0 15px" }}>System Status</h3>
        <div style={{ display: "grid", gap: "10px", gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))" }}>
          <StatusItem label="Authentication" status="online" />
          <StatusItem label="Database" status="online" />
          <StatusItem label="MCP Server" status="online" />
          <StatusItem label="API Gateway" status="online" />
        </div>
      </div>
    </AdminLayout>
  );
}

function DashboardCard({
  title,
  description,
  icon,
  href,
  status,
}: {
  title: string;
  description: string;
  icon: string;
  href: string;
  status: "active" | "coming-soon";
}) {
  return (
    <a
      href={href}
      style={{
        display: "block",
        padding: "24px",
        background: "white",
        borderRadius: "12px",
        textDecoration: "none",
        color: "inherit",
        border: "1px solid #e0e0e0",
        transition: "transform 0.2s, box-shadow 0.2s",
        opacity: status === "coming-soon" ? 0.7 : 1,
        pointerEvents: status === "coming-soon" ? "none" : "auto",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "15px", marginBottom: "10px" }}>
        <span style={{ fontSize: "2rem" }}>{icon}</span>
        <h3 style={{ margin: 0, fontSize: "1.25rem" }}>{title}</h3>
        {status === "coming-soon" && (
          <span
            style={{
              background: "#f59e0b",
              color: "white",
              padding: "2px 8px",
              borderRadius: "4px",
              fontSize: "0.7rem",
              marginLeft: "auto",
            }}
          >
            Coming Soon
          </span>
        )}
      </div>
      <p style={{ margin: 0, color: "#666" }}>{description}</p>
    </a>
  );
}

function StatusItem({ label, status }: { label: string; status: "online" | "offline" }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
      <span
        style={{
          width: "10px",
          height: "10px",
          borderRadius: "50%",
          background: status === "online" ? "#10b981" : "#ef4444",
        }}
      />
      <span>{label}</span>
      <span style={{ marginLeft: "auto", color: status === "online" ? "#10b981" : "#ef4444", fontSize: "0.875rem" }}>
        {status === "online" ? "Online" : "Offline"}
      </span>
    </div>
  );
}
