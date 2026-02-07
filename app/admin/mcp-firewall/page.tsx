"use client";

import { useEffect, useState } from "react";
import AdminLayout from "../components/AdminLayout";

interface MCPFirewallData {
  status: string;
  endpoints: {
    name: string;
    url: string;
    status: "active" | "inactive";
    requests: number;
    lastChecked: string;
  }[];
  threats: {
    id: string;
    type: string;
    severity: "low" | "medium" | "high";
    timestamp: string;
    details: string;
  }[];
  metrics: {
    totalRequests: number;
    blockedRequests: number;
    allowedRequests: number;
    uniqueAgents: number;
  };
}

export default function MCPFirewallModule() {
  const [data, setData] = useState<MCPFirewallData | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    // Simulate fetching data - in production this would call your API
    fetchFirewallData();
  }, []);

  async function fetchFirewallData() {
    // Mock data for now - replace with actual API call
    setData({
      status: "active",
      endpoints: [
        { name: "MCP Server", url: "/api/mcp", status: "active", requests: 1234, lastChecked: "2 mins ago" },
        { name: "Auth API", url: "/api/auth", status: "active", requests: 567, lastChecked: "1 min ago" },
        { name: "Health Check", url: "/api/health", status: "active", requests: 89, lastChecked: "30 secs ago" },
      ],
      threats: [
        { id: "1", type: "Rate Limit Exceeded", severity: "medium", timestamp: "2026-02-07 10:30", details: "IP 192.168.1.100 exceeded rate limit" },
        { id: "2", type: "Invalid Token", severity: "low", timestamp: "2026-02-07 09:15", details: "Invalid API key attempt" },
      ],
      metrics: {
        totalRequests: 1890,
        blockedRequests: 23,
        allowedRequests: 1867,
        uniqueAgents: 45,
      },
    });
    setLoading(false);
  }

  if (loading) {
    return (
      <AdminLayout title="MCP Firewall">
        <div>Loading firewall data...</div>
      </AdminLayout>
    );
  }

  return (
    <AdminLayout title="MCP Firewall Status">
      {/* Overview Cards */}
      <div style={{ display: "grid", gap: "20px", gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))", marginBottom: "30px" }}>
        <MetricCard title="Total Requests" value={data?.metrics.totalRequests || 0} color="#3b82f6" />
        <MetricCard title="Blocked" value={data?.metrics.blockedRequests || 0} color="#ef4444" />
        <MetricCard title="Allowed" value={data?.metrics.allowedRequests || 0} color="#10b981" />
        <MetricCard title="Unique Agents" value={data?.metrics.uniqueAgents || 0} color="#8b5cf6" />
      </div>

      {/* Endpoints Status */}
      <Section title="API Endpoints">
        <table style={{ width: "100%", borderCollapse: "collapse" }}>
          <thead>
            <tr style={{ background: "#f9fafb", textAlign: "left" }}>
              <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Name</th>
              <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>URL</th>
              <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Status</th>
              <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Requests</th>
              <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Last Checked</th>
            </tr>
          </thead>
          <tbody>
            {data?.endpoints.map((endpoint) => (
              <tr key={endpoint.name}>
                <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>{endpoint.name}</td>
                <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb", fontFamily: "monospace", fontSize: "0.875rem" }}>
                  {endpoint.url}
                </td>
                <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>
                  <StatusBadge status={endpoint.status} />
                </td>
                <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>{endpoint.requests.toLocaleString()}</td>
                <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb", color: "#6b7280" }}>{endpoint.lastChecked}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Section>

      {/* Security Threats */}
      <Section title="Recent Security Events">
        {data?.threats.length === 0 ? (
          <p style={{ color: "#6b7280" }}>No security events recorded.</p>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
            {data?.threats.map((threat) => (
              <div
                key={threat.id}
                style={{
                  padding: "16px",
                  background: threat.severity === "high" ? "#fef2f2" : threat.severity === "medium" ? "#fffbeb" : "#f0fdf4",
                  borderRadius: "8px",
                  border: `1px solid ${threat.severity === "high" ? "#fecaca" : threat.severity === "medium" ? "#fcd34d" : "#bbf7d0"}`,
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "8px" }}>
                  <span style={{ fontWeight: "bold" }}>{threat.type}</span>
                  <SeverityBadge severity={threat.severity} />
                </div>
                <p style={{ margin: "0 0 8px", color: "#4b5563" }}>{threat.details}</p>
                <span style={{ fontSize: "0.875rem", color: "#6b7280" }}>{threat.timestamp}</span>
              </div>
            ))}
          </div>
        )}
      </Section>

      {/* OpenEdison Integration Note */}
      <Section title="OpenEdison Integration">
        <div style={{ padding: "16px", background: "#eff6ff", borderRadius: "8px", border: "1px solid #bfdbfe" }}>
          <p style={{ margin: "0 0 10px" }}>
            <strong>🔗 OpenEdison Connected</strong>
          </p>
          <p style={{ margin: "0", color: "#4b5563" }}>
            Firewall data is being synchronized with OpenEdison for enhanced monitoring and control.
            View detailed analytics and configure firewall rules in the OpenEdison dashboard.
          </p>
          <a
            href="https://github.com/Edison-Watch/open-edison"
            target="_blank"
            rel="noopener noreferrer"
            style={{ display: "inline-block", marginTop: "10px", color: "#2563eb" }}
          >
            View OpenEdison Documentation →
          </a>
        </div>
      </Section>
    </AdminLayout>
  );
}

function MetricCard({ title, value, color }: { title: string; value: number; color: string }) {
  return (
    <div style={{ padding: "20px", background: "white", borderRadius: "8px", border: "1px solid #e5e7eb" }}>
      <p style={{ margin: "0 0 8px", color: "#6b7280", fontSize: "0.875rem" }}>{title}</p>
      <p style={{ margin: 0, fontSize: "2rem", fontWeight: "bold", color }}>{value.toLocaleString()}</p>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: "30px", padding: "20px", background: "white", borderRadius: "8px", border: "1px solid #e5e7eb" }}>
      <h3 style={{ margin: "0 0 20px", fontSize: "1.125rem" }}>{title}</h3>
      {children}
    </div>
  );
}

function StatusBadge({ status }: { status: "active" | "inactive" }) {
  return (
    <span
      style={{
        display: "inline-block",
        padding: "4px 12px",
        borderRadius: "12px",
        fontSize: "0.75rem",
        fontWeight: "bold",
        background: status === "active" ? "#dcfce7" : "#fee2e2",
        color: status === "active" ? "#166534" : "#991b1b",
      }}
    >
      {status === "active" ? "● Active" : "○ Inactive"}
    </span>
  );
}

function SeverityBadge({ severity }: { severity: "low" | "medium" | "high" }) {
  const colors = {
    low: { bg: "#dcfce7", color: "#166534" },
    medium: { bg: "#fef3c7", color: "#92400e" },
    high: { bg: "#fee2e2", color: "#991b1b" },
  };

  return (
    <span
      style={{
        display: "inline-block",
        padding: "4px 12px",
        borderRadius: "12px",
        fontSize: "0.75rem",
        fontWeight: "bold",
        textTransform: "uppercase",
        background: colors[severity].bg,
        color: colors[severity].color,
      }}
    >
      {severity}
    </span>
  );
}
