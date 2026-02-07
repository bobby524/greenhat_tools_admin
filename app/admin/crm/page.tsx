"use client";

import AdminLayout from "../components/AdminLayout";

export default function CRMAdminModule() {
  return (
    <AdminLayout title="CRM Admin">
      {/* Coming Soon Banner */}
      <div
        style={{
          padding: "40px",
          background: "linear-gradient(135deg, #667eea 0%, #764ba2 100%)",
          borderRadius: "12px",
          color: "white",
          textAlign: "center",
          marginBottom: "30px",
        }}
      >
        <h2 style={{ margin: "0 0 15px", fontSize: "1.75rem" }}>🚧 Coming Soon</h2>
        <p style={{ margin: "0", fontSize: "1.1rem", opacity: 0.9 }}>
          The CRM Admin module is currently being migrated from tools.greenhatsec.com
        </p>
      </div>

      {/* Feature Preview */}
      <div style={{ marginBottom: "30px" }}>
        <h3 style={{ margin: "0 0 20px" }}>Planned Features</h3>
        <div style={{ display: "grid", gap: "20px", gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))" }}>
          <FeatureCard
            icon="👥"
            title="User Management"
            description="Manage CRM users, roles, and permissions"
          />
          <FeatureCard
            icon="📊"
            title="Analytics Dashboard"
            description="View CRM metrics and performance analytics"
          />
          <FeatureCard
            icon="🔄"
            title="Data Sync"
            description="Configure data synchronization settings"
          />
          <FeatureCard
            icon="⚙️"
            title="Settings"
            description="Manage CRM configuration and preferences"
          />
          <FeatureCard
            icon="📧"
            title="Email Templates"
            description="Customize email templates and notifications"
          />
          <FeatureCard
            icon="📋"
            title="Custom Fields"
            description="Manage custom fields and data structures"
          />
        </div>
      </div>

      {/* Current Location */}
      <div
        style={{
          padding: "20px",
          background: "#f0fdf4",
          borderRadius: "8px",
          border: "1px solid #bbf7d0",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <div>
          <h4 style={{ margin: "0 0 5px" }}>Looking for CRM Admin?</h4>
          <p style={{ margin: 0, color: "#4b5563" }}>
            The admin settings are currently located at tools.greenhatsec.com
          </p>
        </div>
        <a
          href="https://tools.greenhatsec.com/admin"
          target="_blank"
          rel="noopener noreferrer"
          style={{
            padding: "10px 20px",
            background: "#10b981",
            color: "white",
            borderRadius: "6px",
            textDecoration: "none",
            fontWeight: "bold",
          }}
        >
          Go to CRM Admin →
        </a>
      </div>

      {/* Migration Progress */}
      <div style={{ marginTop: "30px", padding: "20px", background: "white", borderRadius: "8px", border: "1px solid #e5e7eb" }}>
        <h4 style={{ margin: "0 0 15px" }}>Migration Progress</h4>
        <div style={{ marginBottom: "15px" }}>
          <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "5px" }}>
            <span>Overall Progress</span>
            <span style={{ fontWeight: "bold" }}>25%</span>
          </div>
          <div style={{ height: "8px", background: "#e5e7eb", borderRadius: "4px", overflow: "hidden" }}>
            <div style={{ width: "25%", height: "100%", background: "#3b82f6", borderRadius: "4px" }} />
          </div>
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
          <ProgressItem label="UI Components" status="completed" />
          <ProgressItem label="API Integration" status="in-progress" />
          <ProgressItem label="Data Migration" status="pending" />
          <ProgressItem label="Testing" status="pending" />
        </div>
      </div>
    </AdminLayout>
  );
}

function FeatureCard({
  icon,
  title,
  description,
}: {
  icon: string;
  title: string;
  description: string;
}) {
  return (
    <div
      style={{
        padding: "20px",
        background: "white",
        borderRadius: "8px",
        border: "1px solid #e5e7eb",
        opacity: 0.7,
      }}
    >
      <div style={{ fontSize: "2rem", marginBottom: "10px" }}>{icon}</div>
      <h4 style={{ margin: "0 0 8px" }}>{title}</h4>
      <p style={{ margin: 0, color: "#6b7280", fontSize: "0.875rem" }}>{description}</p>
    </div>
  );
}

function ProgressItem({ label, status }: { label: string; status: "completed" | "in-progress" | "pending" }) {
  const statusColors = {
    completed: "#10b981",
    "in-progress": "#3b82f6",
    pending: "#9ca3af",
  };

  const statusIcons = {
    completed: "✓",
    "in-progress": "◐",
    pending: "○",
  };

  return (
    <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
      <span style={{ color: statusColors[status], fontWeight: "bold" }}>{statusIcons[status]}</span>
      <span style={{ color: status === "pending" ? "#9ca3af" : "inherit" }}>{label}</span>
      <span
        style={{
          marginLeft: "auto",
          fontSize: "0.75rem",
          padding: "2px 8px",
          borderRadius: "4px",
          background: statusColors[status] + "20",
          color: statusColors[status],
          textTransform: "capitalize",
        }}
      >
        {status.replace("-", " ")}
      </span>
    </div>
  );
}
