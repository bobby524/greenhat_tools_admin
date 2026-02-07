"use client";

import { useEffect, useState, useCallback } from "react";
import AdminLayout from "../components/AdminLayout";

interface AuditLog {
  id: string;
  timestamp: string;
  sessionId: string;
  toolName: string;
  params: any;
  result: "success" | "error" | "blocked";
  error?: string;
  durationMs: number;
  aclLevel: "PUBLIC" | "PRIVATE" | "SECRET";
  riskFlags: {
    readPrivateData: boolean;
    writeOperation: boolean;
    externalCommunication: boolean;
  };
}

interface SessionMetrics {
  sessionId: string;
  startTime: string;
  toolCalls: number;
  errors: number;
  blocked: number;
  maxAclLevel: "PUBLIC" | "PRIVATE" | "SECRET";
  lethalTrifecta: boolean;
}

interface SecurityStatus {
  session: SessionMetrics;
  recentLogs: AuditLog[];
  riskLevel: "low" | "medium" | "high" | "critical";
  alerts: string[];
  firewall: {
    enabled: boolean;
    defaultPolicy: string;
    toolsConfigured: number;
    blockedSessions: number;
    dataLeakPrevention: boolean;
    lethalTrifectaProtection: boolean;
  };
  allSessions: SessionMetrics[];
}

export default function MCPFirewallModule() {
  const [status, setStatus] = useState<SecurityStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [autoRefresh, setAutoRefresh] = useState(true);

  const fetchData = useCallback(async () => {
    try {
      const response = await fetch("/api/mcp-proxy/dashboard?limit=100", {
        cache: "no-store",
      });
      
      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }
      
      const data = await response.json();
      setStatus(data);
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to fetch data";
      setError(message);
      console.error("Error fetching MCP data:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    
    // Auto-refresh every 5 seconds
    let interval: NodeJS.Timeout;
    if (autoRefresh) {
      interval = setInterval(fetchData, 5000);
    }
    
    return () => {
      if (interval) clearInterval(interval);
    };
  }, [fetchData, autoRefresh]);

  if (loading) {
    return (
      <AdminLayout title="MCP Firewall">
        <div style={{ padding: "40px", textAlign: "center" }}>
          <div>Loading firewall data...</div>
          <div style={{ marginTop: "10px", fontSize: "0.875rem", color: "#6b7280" }}>
            Connecting to MCP server...
          </div>
        </div>
      </AdminLayout>
    );
  }

  if (error) {
    return (
      <AdminLayout title="MCP Firewall">
        <div style={{ padding: "40px", textAlign: "center", color: "#dc2626" }}>
          <h3>Error Loading Data</h3>
          <p>{error}</p>
          <button
            onClick={fetchData}
            style={{
              marginTop: "20px",
              padding: "10px 20px",
              background: "#2563eb",
              color: "white",
              border: "none",
              borderRadius: "6px",
              cursor: "pointer",
            }}
          >
            Retry
          </button>
        </div>
      </AdminLayout>
    );
  }

  if (!status) {
    return (
      <AdminLayout title="MCP Firewall">
        <div style={{ padding: "40px", textAlign: "center" }}>
          No data available
        </div>
      </AdminLayout>
    );
  }

  const totalCalls = status.allSessions.reduce((sum, s) => sum + s.toolCalls, 0);
  const totalErrors = status.allSessions.reduce((sum, s) => sum + s.errors, 0);
  const totalBlocked = status.allSessions.reduce((sum, s) => sum + s.blocked, 0);
  const uniqueSessions = status.allSessions.length;

  return (
    <AdminLayout title="MCP Firewall Status">
      {/* Header with Auto-refresh Toggle */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "20px" }}>
        <div>
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: "8px",
              padding: "6px 12px",
              borderRadius: "20px",
              fontSize: "0.875rem",
              fontWeight: "bold",
              background: status.firewall.enabled ? "#dcfce7" : "#fee2e2",
              color: status.firewall.enabled ? "#166534" : "#991b1b",
            }}
          >
            <span>{status.firewall.enabled ? "🟢" : "🔴"}</span>
            Firewall {status.firewall.enabled ? "Enabled" : "Disabled"}
          </span>
          <span style={{ marginLeft: "10px", fontSize: "0.875rem", color: "#6b7280" }}>
            Risk Level: 
            <RiskBadge level={status.riskLevel} />
          </span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
          <label style={{ display: "flex", alignItems: "center", gap: "5px", fontSize: "0.875rem", cursor: "pointer" }}>
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
            />
            Auto-refresh
          </label>
          <button
            onClick={fetchData}
            style={{
              padding: "6px 12px",
              background: "#f3f4f6",
              border: "1px solid #d1d5db",
              borderRadius: "4px",
              cursor: "pointer",
              fontSize: "0.875rem",
            }}
          >
            🔄 Refresh
          </button>
        </div>
      </div>

      {/* Overview Cards */}
      <div style={{ display: "grid", gap: "20px", gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))", marginBottom: "30px" }}>
        <MetricCard title="Total Tool Calls" value={totalCalls} color="#3b82f6" />
        <MetricCard title="Errors" value={totalErrors} color="#ef4444" />
        <MetricCard title="Blocked" value={totalBlocked} color="#f59e0b" />
        <MetricCard title="Active Sessions" value={uniqueSessions} color="#8b5cf6" />
      </div>

      {/* Firewall Configuration */}
      <Section title="Firewall Configuration">
        <div style={{ display: "grid", gap: "15px", gridTemplateColumns: "repeat(auto-fit, minmax(250px, 1fr))" }}>
          <ConfigItem label="Default Policy" value={status.firewall.defaultPolicy} />
          <ConfigItem label="Tools Configured" value={status.firewall.toolsConfigured.toString()} />
          <ConfigItem label="Blocked Sessions" value={status.firewall.blockedSessions.toString()} />
          <ConfigItem label="Data Leak Prevention" value={status.firewall.dataLeakPrevention ? "✓ Enabled" : "✗ Disabled"} />
          <ConfigItem label="Lethal Trifecta Protection" value={status.firewall.lethalTrifectaProtection ? "✓ Enabled" : "✗ Disabled"} />
        </div>
      </Section>

      {/* Active Sessions */}
      <Section title="Active Sessions">
        {status.allSessions.length === 0 ? (
          <p style={{ color: "#6b7280" }}>No active sessions</p>
        ) : (
          <table style={{ width: "100%", borderCollapse: "collapse" }}>
            <thead>
              <tr style={{ background: "#f9fafb", textAlign: "left" }}>
                <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Session ID</th>
                <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Started</th>
                <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Tool Calls</th>
                <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Errors</th>
                <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Blocked</th>
                <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>ACL Level</th>
                <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Risk</th>
              </tr>
            </thead>
            <tbody>
              {status.allSessions.map((session) => (
                <tr key={session.sessionId}>
                  <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb", fontFamily: "monospace", fontSize: "0.75rem" }}>
                    {session.sessionId.substring(0, 8)}...
                  </td>
                  <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb", fontSize: "0.875rem" }}>
                    {new Date(session.startTime).toLocaleTimeString()}
                  </td>
                  <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>{session.toolCalls}</td>
                  <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb", color: session.errors > 0 ? "#dc2626" : "inherit" }}>
                    {session.errors}
                  </td>
                  <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb", color: session.blocked > 0 ? "#f59e0b" : "inherit" }}>
                    {session.blocked}
                  </td>
                  <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>
                    <AclBadge level={session.maxAclLevel} />
                  </td>
                  <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>
                    {session.lethalTrifecta ? <span style={{ color: "#dc2626", fontWeight: "bold" }}>⚠️ HIGH</span> : "✓ Normal"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Section>

      {/* Recent Audit Logs */}
      <Section title="Recent Activity">
        {status.recentLogs.length === 0 ? (
          <p style={{ color: "#6b7280" }}>No recent activity</p>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
            {status.recentLogs.slice(0, 20).map((log) => (
              <div
                key={log.id}
                style={{
                  padding: "12px 16px",
                  background: log.result === "blocked" ? "#fef2f2" : log.result === "error" ? "#fffbeb" : "#f0fdf4",
                  borderRadius: "8px",
                  border: `1px solid ${log.result === "blocked" ? "#fecaca" : log.result === "error" ? "#fcd34d" : "#bbf7d0"}`,
                }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "8px" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                    <span style={{ fontWeight: "bold" }}>{log.toolName}</span>
                    <ResultBadge result={log.result} />
                    <AclBadge level={log.aclLevel} />
                  </div>
                  <span style={{ fontSize: "0.75rem", color: "#6b7280" }}>
                    {new Date(log.timestamp).toLocaleString()}
                  </span>
                </div>
                <div style={{ display: "flex", gap: "10px", fontSize: "0.875rem", color: "#4b5563" }}>
                  <span>Session: {log.sessionId.substring(0, 8)}...</span>
                  <span>Duration: {log.durationMs}ms</span>
                  {log.riskFlags.writeOperation && <span style={{ color: "#dc2626" }}>✏️ Write</span>}
                  {log.riskFlags.readPrivateData && <span style={{ color: "#f59e0b" }}>🔒 Private Data</span>}
                  {log.riskFlags.externalCommunication && <span style={{ color: "#8b5cf6" }}>🌐 External</span>}
                </div>
                {log.error && (
                  <div style={{ marginTop: "8px", padding: "8px", background: "#fee2e2", borderRadius: "4px", fontSize: "0.75rem", color: "#991b1b" }}>
                    Error: {log.error}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </Section>

      {/* Alerts */}
      {status.alerts.length > 0 && (
        <Section title="Security Alerts">
          <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
            {status.alerts.map((alert, index) => (
              <div
                key={index}
                style={{
                  padding: "12px 16px",
                  background: "#fef2f2",
                  borderRadius: "8px",
                  border: "1px solid #fecaca",
                  color: "#991b1b",
                }}
              >
                ⚠️ {alert}
              </div>
            ))}
          </div>
        </Section>
      )}
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

function ConfigItem({ label, value }: { label: string; value: string }) {
  return (
    <div style={{ display: "flex", justifyContent: "space-between", padding: "10px", background: "#f9fafb", borderRadius: "6px" }}>
      <span style={{ color: "#6b7280" }}>{label}</span>
      <span style={{ fontWeight: "bold" }}>{value}</span>
    </div>
  );
}

function RiskBadge({ level }: { level: "low" | "medium" | "high" | "critical" }) {
  const colors = {
    low: { bg: "#dcfce7", color: "#166534" },
    medium: { bg: "#fef3c7", color: "#92400e" },
    high: { bg: "#fee2e2", color: "#991b1b" },
    critical: { bg: "#fecaca", color: "#7f1d1d" },
  };

  const { bg, color } = colors[level] || colors.low;

  return (
    <span
      style={{
        marginLeft: "5px",
        padding: "2px 8px",
        borderRadius: "4px",
        fontSize: "0.75rem",
        fontWeight: "bold",
        textTransform: "uppercase",
        background: bg,
        color: color,
      }}
    >
      {level}
    </span>
  );
}

function AclBadge({ level }: { level: "PUBLIC" | "PRIVATE" | "SECRET" }) {
  const colors = {
    PUBLIC: { bg: "#dcfce7", color: "#166534" },
    PRIVATE: { bg: "#fef3c7", color: "#92400e" },
    SECRET: { bg: "#fee2e2", color: "#991b1b" },
  };

  const { bg, color } = colors[level] || colors.PUBLIC;

  return (
    <span
      style={{
        padding: "2px 8px",
        borderRadius: "4px",
        fontSize: "0.625rem",
        fontWeight: "bold",
        background: bg,
        color: color,
      }}
    >
      {level}
    </span>
  );
}

function ResultBadge({ result }: { result: "success" | "error" | "blocked" }) {
  const colors = {
    success: { bg: "#dcfce7", color: "#166534" },
    error: { bg: "#fee2e2", color: "#991b1b" },
    blocked: { bg: "#fef3c7", color: "#92400e" },
  };

  const { bg, color } = colors[result] || colors.success;

  return (
    <span
      style={{
        padding: "2px 8px",
        borderRadius: "4px",
        fontSize: "0.625rem",
        fontWeight: "bold",
        textTransform: "uppercase",
        background: bg,
        color: color,
      }}
    >
      {result}
    </span>
  );
}
