"use client";

import { useState, useEffect, useCallback, useRef, useMemo } from "react";

// Types
interface ToolPermission {
  enabled: boolean;
  writeOperation: boolean;
  readPrivateData: boolean;
  readUntrustedPublicData: boolean;
  externalCommunication: boolean;
  acl: "SECRET" | "PRIVATE" | "PUBLIC";
  rateLimit?: {
    requestsPerMinute: number;
    requestsPerHour: number;
  };
}

interface FirewallConfig {
  defaultPolicy: "allow" | "deny";
  enableRateLimiting: boolean;
  enableDataLeakPrevention: boolean;
  enableLethalTrifectaProtection: boolean;
  toolPermissions: Record<string, ToolPermission>;
  blockedPatterns: string[];
}

interface Session {
  id: string;
  createdAt: string;
  toolCalls: ToolCall[];
  userAgent?: string;
  ip?: string;
  status: "active" | "closed" | "blocked";
}

interface ToolCall {
  id: string;
  tool: string;
  timestamp: string;
  status: "success" | "error" | "blocked" | "pending";
  duration?: number;
  error?: string;
}

interface AuditLog {
  id: string;
  timestamp: string;
  sessionId: string;
  tool: string;
  action: "call" | "block" | "error" | "auth" | "config_change";
  status: "success" | "blocked" | "error" | "warning";
  details?: string;
  ip?: string;
  metadata?: Record<string, any>;
}

// Debounce hook
function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);
  useEffect(() => {
    const handler = setTimeout(() => setDebouncedValue(value), delay);
    return () => clearTimeout(handler);
  }, [value, delay]);
  return debouncedValue;
}

// Format relative time
function formatRelativeTime(timestamp: string): string {
  const diff = Date.now() - new Date(timestamp).getTime();
  const seconds = Math.floor(diff / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  
  if (seconds < 60) return `${seconds}s ago`;
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  return new Date(timestamp).toLocaleDateString();
}

export default function FirewallDashboard() {
  // State
  const [activeTab, setActiveTab] = useState<"overview" | "sessions" | "audit" | "permissions">("overview");
  const [config, setConfig] = useState<FirewallConfig | null>(null);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [auditLogs, setAuditLogs] = useState<AuditLog[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date>(new Date());
  const [pendingUpdates, setPendingUpdates] = useState<number>(0);
  
  // Pending changes for batching
  const pendingChangesRef = useRef<Map<string, Partial<ToolPermission>>>(new Map());
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false);

  // Fetch all data
  const fetchData = useCallback(async () => {
    try {
      const [configRes, sessionsRes, auditRes] = await Promise.all([
        fetch("/api/firewall"),
        fetch("/api/sessions"),
        fetch("/api/audit?limit=20"),
      ]);

      if (configRes.ok) {
        const configData = await configRes.json();
        setConfig(configData.config);
      }

      if (sessionsRes.ok) {
        const sessionsData = await sessionsRes.json();
        setSessions(sessionsData.sessions);
      }

      if (auditRes.ok) {
        const auditData = await auditRes.json();
        setAuditLogs(auditData.logs);
      }

      setLastUpdated(new Date());
      setError(null);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial load and polling
  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 5000); // Poll every 5 seconds
    return () => clearInterval(interval);
  }, [fetchData]);

  // Batch save changes
  const saveChanges = useCallback(async () => {
    if (pendingChangesRef.current.size === 0) return;

    setPendingUpdates((prev) => prev + 1);
    try {
      const updates = Object.fromEntries(pendingChangesRef.current);
      
      const res = await fetch("/api/firewall", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ toolPermissions: updates }),
      });

      if (res.ok) {
        pendingChangesRef.current.clear();
        setHasUnsavedChanges(false);
        await fetchData();
      }
    } catch (err: any) {
      setError(err.message);
    } finally {
      setPendingUpdates((prev) => prev - 1);
    }
  }, [fetchData]);

  // Debounced save
  const debouncedSave = useDebounce(saveChanges, 1000);

  // Update tool permission
  const updateToolPermission = useCallback((toolName: string, updates: Partial<ToolPermission>) => {
    pendingChangesRef.current.set(toolName, {
      ...pendingChangesRef.current.get(toolName),
      ...updates,
    });
    setHasUnsavedChanges(true);
    debouncedSave();
  }, [debouncedSave]);

  // Toggle tool enabled
  const toggleTool = useCallback((toolName: string) => {
    const current = config?.toolPermissions[toolName];
    if (current) {
      updateToolPermission(toolName, { enabled: !current.enabled });
      // Optimistic update
      setConfig((prev) => {
        if (!prev) return prev;
        return {
          ...prev,
          toolPermissions: {
            ...prev.toolPermissions,
            [toolName]: { ...current, enabled: !current.enabled },
          },
        };
      });
    }
  }, [config, updateToolPermission]);

  // Update rate limit
  const updateRateLimit = useCallback((toolName: string, field: "requestsPerMinute" | "requestsPerHour", value: number) => {
    const current = config?.toolPermissions[toolName];
    if (current) {
      const newRateLimit = {
        ...(current.rateLimit || { requestsPerMinute: 0, requestsPerHour: 0 }),
        [field]: value,
      };
      updateToolPermission(toolName, { rateLimit: newRateLimit });
      // Optimistic update
      setConfig((prev) => {
        if (!prev) return prev;
        return {
          ...prev,
          toolPermissions: {
            ...prev.toolPermissions,
            [toolName]: { ...current, rateLimit: newRateLimit },
          },
        };
      });
    }
  }, [config, updateToolPermission]);

  // Update global setting
  const updateGlobalSetting = useCallback(async (key: keyof FirewallConfig, value: any) => {
    try {
      setPendingUpdates((prev) => prev + 1);
      const res = await fetch("/api/firewall", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ globalSettings: { [key]: value } }),
      });

      if (res.ok) {
        setConfig((prev) => prev ? { ...prev, [key]: value } : prev);
      }
    } catch (err: any) {
      setError(err.message);
    } finally {
      setPendingUpdates((prev) => prev - 1);
    }
  }, []);

  // Stats
  const stats = useMemo(() => {
    const activeSessions = sessions.filter((s) => s.status === "active").length;
    const blockedCalls = sessions.reduce(
      (acc, s) => acc + s.toolCalls.filter((tc) => tc.status === "blocked").length,
      0
    );
    const totalCalls = sessions.reduce((acc, s) => acc + s.toolCalls.length, 0);
    const recentBlocks = auditLogs.filter(
      (l) => l.status === "blocked" && new Date(l.timestamp).getTime() > Date.now() - 3600000
    ).length;

    return { activeSessions, blockedCalls, totalCalls, recentBlocks };
  }, [sessions, auditLogs]);

  // Group tools by category
  const groupedTools = useMemo(() => {
    if (!config) return {};
    return Object.entries(config.toolPermissions).reduce(
      (acc, [name, permission]) => {
        const category = name.split("_")[0];
        if (!acc[category]) acc[category] = [];
        acc[category].push({ name, permission });
        return acc;
      },
      {} as Record<string, { name: string; permission: ToolPermission }[]>
    );
  }, [config]);

  if (loading && !config) {
    return (
      <div style={{ padding: "40px", textAlign: "center" }}>
        <div style={{ fontSize: "24px", marginBottom: "16px" }}>🛡️</div>
        <p>Loading firewall dashboard...</p>
      </div>
    );
  }

  return (
    <div style={{ padding: "24px", maxWidth: "1400px", margin: "0 auto" }}>
      {/* Header */}
      <div style={{ marginBottom: "24px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "8px" }}>
          <h1 style={{ margin: 0 }}>🛡️ MCP Firewall</h1>
          <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
            {pendingUpdates > 0 && (
              <span style={{ color: "#666", fontSize: "14px" }}>💾 Saving...</span>
            )}
            {hasUnsavedChanges && (
              <span style={{ color: "#d97706", fontSize: "14px" }}>⚠️ Unsaved changes</span>
            )}
            <span style={{ color: "#666", fontSize: "14px" }}>
              Last updated: {lastUpdated.toLocaleTimeString()}
            </span>
            <button
              onClick={fetchData}
              style={{
                padding: "6px 12px",
                background: "#1a1a2e",
                color: "white",
                border: "none",
                borderRadius: "4px",
                cursor: "pointer",
              }}
            >
              🔄 Refresh
            </button>
          </div>
        </div>
        {error && (
          <div style={{ padding: "12px", background: "#fee2e2", color: "#dc2626", borderRadius: "4px", marginTop: "8px" }}>
            Error: {error}
          </div>
        )}
      </div>

      {/* Navigation */}
      <div style={{ display: "flex", gap: "8px", marginBottom: "24px", borderBottom: "1px solid #ddd", paddingBottom: "8px" }}>
        {[
          { key: "overview", label: "Overview", icon: "📊" },
          { key: "sessions", label: "Active Sessions", icon: "👥" },
          { key: "audit", label: "Audit Logs", icon: "📜" },
          { key: "permissions", label: "Tool Permissions", icon: "🔐" },
        ].map((tab) => (
          <button
            key={tab.key}
            onClick={() => setActiveTab(tab.key as any)}
            style={{
              padding: "8px 16px",
              background: activeTab === tab.key ? "#1a1a2e" : "transparent",
              color: activeTab === tab.key ? "white" : "#333",
              border: "none",
              borderRadius: "4px",
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              gap: "6px",
            }}
          >
            {tab.icon} {tab.label}
          </button>
        ))}
      </div>

      {/* Overview Tab */}
      {activeTab === "overview" && (
        <div style={{ display: "grid", gap: "24px" }}>
          {/* Stats Cards */}
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))", gap: "16px" }}>
            <StatCard title="Active Sessions" value={stats.activeSessions} color="#3b82f6" icon="👥" />
            <StatCard title="Total Tool Calls" value={stats.totalCalls} color="#10b981" icon="🔧" />
            <StatCard title="Blocked Calls" value={stats.blockedCalls} color="#dc2626" icon="🚫" />
            <StatCard title="Recent Blocks (1h)" value={stats.recentBlocks} color="#d97706" icon="⚠️" />
          </div>

          {/* Global Settings */}
          <div style={{ padding: "24px", background: "#f9fafb", borderRadius: "8px" }}>
            <h3 style={{ marginTop: 0 }}>🔒 Global Security Settings</h3>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(300px, 1fr))", gap: "16px", marginTop: "16px" }}>
              <ToggleSetting
                label="Enable Rate Limiting"
                description="Limit tool calls per minute/hour per session"
                checked={config?.enableRateLimiting ?? true}
                onChange={(v) => updateGlobalSetting("enableRateLimiting", v)}
              />
              <ToggleSetting
                label="Enable Data Leak Prevention"
                description="Block attempts to exfiltrate private data"
                checked={config?.enableDataLeakPrevention ?? true}
                onChange={(v) => updateGlobalSetting("enableDataLeakPrevention", v)}
              />
              <ToggleSetting
                label="Enable Lethal Trifecta Protection"
                description="Block dangerous combinations (write + private + external)"
                checked={config?.enableLethalTrifectaProtection ?? true}
                onChange={(v) => updateGlobalSetting("enableLethalTrifectaProtection", v)}
              />
            </div>
          </div>

          {/* Blocked Patterns */}
          <div style={{ padding: "24px", background: "#fef2f2", borderRadius: "8px" }}>
            <h3 style={{ marginTop: 0, color: "#dc2626" }}>🚫 Blocked Patterns</h3>
            <div style={{ display: "flex", flexWrap: "wrap", gap: "8px", marginTop: "12px" }}>
              {config?.blockedPatterns.map((pattern) => (
                <span
                  key={pattern}
                  style={{
                    padding: "4px 12px",
                    background: "#fee2e2",
                    color: "#dc2626",
                    borderRadius: "4px",
                    fontSize: "13px",
                    fontFamily: "monospace",
                  }}
                >
                  {pattern}
                </span>
              ))}
            </div>
          </div>

          {/* Recent Activity */}
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(400px, 1fr))", gap: "24px" }}>
            <div style={{ padding: "24px", background: "#f9fafb", borderRadius: "8px" }}>
              <h3 style={{ marginTop: 0 }}>📝 Recent Audit Logs</h3>
              <div style={{ marginTop: "12px" }}>
                {auditLogs.slice(0, 5).map((log) => (
                  <div key={log.id} style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>
                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: "13px" }}>
                      <span style={{ fontWeight: 500 }}>{log.tool}</span>
                      <span style={{ color: "#666" }}>{formatRelativeTime(log.timestamp)}</span>
                    </div>
                    <div style={{ fontSize: "13px", color: "#666", marginTop: "4px" }}>{log.details}</div>
                    <StatusBadge status={log.status} />
                  </div>
                ))}
              </div>
            </div>

            <div style={{ padding: "24px", background: "#f9fafb", borderRadius: "8px" }}>
              <h3 style={{ marginTop: 0 }}>👥 Active Sessions</h3>
              <div style={{ marginTop: "12px" }}>
                {sessions.filter((s) => s.status === "active").slice(0, 5).map((session) => (
                  <div key={session.id} style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>
                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: "13px" }}>
                      <span style={{ fontWeight: 500, fontFamily: "monospace" }}>{session.id}</span>
                      <StatusBadge status={session.status} />
                    </div>
                    <div style={{ fontSize: "13px", color: "#666", marginTop: "4px" }}>
                      {session.toolCalls.length} tool calls • {formatRelativeTime(session.createdAt)}
                    </div>
                  </div>
                ))}
                {sessions.filter((s) => s.status === "active").length === 0 && (
                  <div style={{ color: "#666", fontSize: "14px" }}>No active sessions</div>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Sessions Tab */}
      {activeTab === "sessions" && (
        <div>
          <h2>Active Sessions ({sessions.filter((s) => s.status === "active").length})</h2>
          <div style={{ display: "grid", gap: "16px", marginTop: "16px" }}>
            {sessions.map((session) => (
              <div key={session.id} style={{ padding: "20px", border: "1px solid #ddd", borderRadius: "8px", background: "white" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <div>
                    <div style={{ fontWeight: 600, fontFamily: "monospace" }}>{session.id}</div>
                    <div style={{ fontSize: "13px", color: "#666", marginTop: "4px" }}>
                      Started {formatRelativeTime(session.createdAt)} • {session.toolCalls.length} calls
                    </div>
                  </div>
                  <StatusBadge status={session.status} />
                </div>
                
                {session.toolCalls.length > 0 && (
                  <div style={{ marginTop: "16px" }}>
                    <div style={{ fontSize: "13px", fontWeight: 500, marginBottom: "8px" }}>Recent Calls:</div>
                    <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                      {session.toolCalls.slice(-5).map((call) => (
                        <div key={call.id} style={{ display: "flex", alignItems: "center", gap: "12px", fontSize: "13px" }}>
                          <StatusBadge status={call.status} />
                          <span style={{ fontFamily: "monospace" }}>{call.tool}</span>
                          <span style={{ color: "#666" }}>{formatRelativeTime(call.timestamp)}</span>
                          {call.duration && <span style={{ color: "#666" }}>({call.duration}ms)</span>}
                          {call.error && <span style={{ color: "#dc2626" }}>{call.error}</span>}
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Audit Tab */}
      {activeTab === "audit" && (
        <div>
          <h2>Audit Logs ({auditLogs.length})</h2>
          <div style={{ marginTop: "16px" }}>
            {auditLogs.map((log) => (
              <div key={log.id} style={{ padding: "16px", borderBottom: "1px solid #e5e7eb", background: "white" }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
                  <div>
                    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                      <StatusBadge status={log.status} />
                      <span style={{ fontWeight: 500 }}>{log.tool}</span>
                      <span style={{ fontSize: "13px", color: "#666" }}>({log.action})</span>
                    </div>
                    <div style={{ fontSize: "13px", color: "#666", marginTop: "4px" }}>{log.details}</div>
                    {log.metadata && (
                      <div style={{ fontSize: "12px", color: "#888", marginTop: "4px", fontFamily: "monospace" }}>
                        {JSON.stringify(log.metadata)}
                      </div>
                    )}
                  </div>
                  <div style={{ textAlign: "right", fontSize: "13px" }}>
                    <div style={{ color: "#666" }}>{formatRelativeTime(log.timestamp)}</div>
                    <div style={{ color: "#999", fontSize: "12px" }}>{log.sessionId}</div>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Permissions Tab */}
      {activeTab === "permissions" && config && (
        <div>
          <h2>Tool Permissions</h2>
          <div style={{ marginTop: "16px" }}>
            {Object.entries(groupedTools).map(([category, tools]) => (
              <div key={category} style={{ marginBottom: "32px" }}>
                <h3 style={{ textTransform: "capitalize", marginBottom: "16px" }}>{category} Tools</h3>
                <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(350px, 1fr))", gap: "16px" }}>
                  {tools.map(({ name, permission }) => (
                    <div key={name} style={{ padding: "20px", border: "1px solid #ddd", borderRadius: "8px", background: permission.enabled ? "white" : "#f9fafb" }}>
                      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "12px" }}>
                        <h4 style={{ margin: 0, fontSize: "14px", fontFamily: "monospace" }}>{name}</h4>
                        <label style={{ display: "flex", alignItems: "center", gap: "6px", cursor: "pointer" }}>
                          <input
                            type="checkbox"
                            checked={permission.enabled}
                            onChange={() => toggleTool(name)}
                          />
                          <span style={{ fontSize: "13px" }}>Enabled</span>
                        </label>
                      </div>

                      {/* Capability Tags */}
                      <div style={{ display: "flex", flexWrap: "wrap", gap: "6px", marginBottom: "12px" }}>
                        {permission.writeOperation && <Tag color="#d97706" label="Write" />}
                        {permission.readPrivateData && <Tag color="#2563eb" label="Private Data" />}
                        {permission.readUntrustedPublicData && <Tag color="#7c3aed" label="Public Data" />}
                        {permission.externalCommunication && <Tag color="#db2777" label="External" />}
                        <Tag color="#dc2626" label={permission.acl} />
                      </div>

                      {/* Rate Limits */}
                      {permission.rateLimit && (
                        <div style={{ padding: "12px", background: "#f9fafb", borderRadius: "6px" }}>
                          <div style={{ fontSize: "12px", fontWeight: 500, marginBottom: "8px" }}>Rate Limits</div>
                          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "12px" }}>
                            <div>
                              <label style={{ fontSize: "12px", color: "#666" }}>Per Minute</label>
                              <input
                                type="number"
                                value={permission.rateLimit.requestsPerMinute}
                                onChange={(e) => updateRateLimit(name, "requestsPerMinute", parseInt(e.target.value) || 0)}
                                style={{ width: "100%", padding: "6px", marginTop: "4px", border: "1px solid #ddd", borderRadius: "4px" }}
                              />
                            </div>
                            <div>
                              <label style={{ fontSize: "12px", color: "#666" }}>Per Hour</label>
                              <input
                                type="number"
                                value={permission.rateLimit.requestsPerHour}
                                onChange={(e) => updateRateLimit(name, "requestsPerHour", parseInt(e.target.value) || 0)}
                                style={{ width: "100%", padding: "6px", marginTop: "4px", border: "1px solid #ddd", borderRadius: "4px" }}
                              />
                            </div>
                          </div>
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// Sub-components
function StatCard({ title, value, color, icon }: { title: string; value: number; color: string; icon: string }) {
  return (
    <div style={{ padding: "20px", background: "white", borderRadius: "8px", border: "1px solid #e5e7eb" }}>
      <div style={{ fontSize: "24px", marginBottom: "8px" }}>{icon}</div>
      <div style={{ fontSize: "28px", fontWeight: 700, color }}>{value}</div>
      <div style={{ fontSize: "14px", color: "#666" }}>{title}</div>
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const colors: Record<string, { bg: string; text: string }> = {
    active: { bg: "#dbeafe", text: "#2563eb" },
    success: { bg: "#d1fae5", text: "#059669" },
    blocked: { bg: "#fee2e2", text: "#dc2626" },
    error: { bg: "#fee2e2", text: "#dc2626" },
    warning: { bg: "#fef3c7", text: "#d97706" },
    pending: { bg: "#f3f4f6", text: "#6b7280" },
    closed: { bg: "#f3f4f6", text: "#6b7280" },
  };
  const { bg, text } = colors[status] || colors.pending;
  
  return (
    <span style={{ padding: "2px 8px", background: bg, color: text, borderRadius: "4px", fontSize: "12px", fontWeight: 500 }}>
      {status}
    </span>
  );
}

function Tag({ color, label }: { color: string; label: string }) {
  return (
    <span style={{ padding: "2px 8px", background: `${color}20`, color, borderRadius: "4px", fontSize: "11px", fontWeight: 500 }}>
      {label}
    </span>
  );
}

function ToggleSetting({ label, description, checked, onChange }: { label: string; description: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label style={{ display: "flex", alignItems: "flex-start", gap: "12px", padding: "16px", background: "white", borderRadius: "6px", border: "1px solid #e5e7eb", cursor: "pointer" }}>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} style={{ marginTop: "2px" }} />
      <div>
        <div style={{ fontWeight: 500 }}>{label}</div>
        <div style={{ fontSize: "13px", color: "#666", marginTop: "2px" }}>{description}</div>
      </div>
    </label>
  );
}