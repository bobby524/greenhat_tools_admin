"use client";

import { useEffect, useState, useCallback } from "react";
import AdminLayout from "../components/AdminLayout";
import { 
  Shield, 
  Activity, 
  AlertTriangle, 
  CheckCircle, 
  XCircle, 
  RefreshCw,
  Clock,
  Lock,
  Globe,
  Edit3,
  AlertOctagon
} from "lucide-react";

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
        <div className="flex flex-col items-center justify-center min-h-[400px]">
          <div className="w-8 h-8 border-2 border-gray-300 border-t-[#62ac4a] rounded-full animate-spin mb-4" />
          <p className="text-gray-600">Loading firewall data...</p>
          <p className="text-sm text-gray-400 mt-1">Connecting to MCP server...</p>
        </div>
      </AdminLayout>
    );
  }

  if (error) {
    return (
      <AdminLayout title="MCP Firewall">
        <div className="flex flex-col items-center justify-center min-h-[400px]">
          <div className="w-12 h-12 bg-red-100 rounded-xl flex items-center justify-center mb-4">
            <XCircle className="w-6 h-6 text-red-600" />
          </div>
          <h3 className="text-lg font-semibold text-gray-900 mb-2">Error Loading Data</h3>
          <p className="text-gray-600 mb-6">{error}</p>
          <button
            onClick={fetchData}
            className="inline-flex items-center gap-2 px-4 py-2 bg-[#62ac4a] text-white rounded-lg hover:bg-[#4e8a3a] transition font-medium"
          >
            <RefreshCw className="w-4 h-4" />
            Retry
          </button>
        </div>
      </AdminLayout>
    );
  }

  if (!status) {
    return (
      <AdminLayout title="MCP Firewall">
        <div className="flex items-center justify-center min-h-[400px] text-gray-500">
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
      {/* Header with Status */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-6">
        <div className="flex flex-wrap items-center gap-3">
          <span className={`inline-flex items-center gap-2 px-3 py-1.5 rounded-full text-sm font-semibold ${
            status.firewall.enabled 
              ? "bg-green-100 text-green-800 border border-green-200" 
              : "bg-red-100 text-red-800 border border-red-200"
          }`}>
            {status.firewall.enabled ? (
              <CheckCircle className="w-4 h-4" />
            ) : (
              <XCircle className="w-4 h-4" />
            )}
            Firewall {status.firewall.enabled ? "Enabled" : "Disabled"}
          </span>
          <span className="text-sm text-gray-500">
            Risk Level: <RiskBadge level={status.riskLevel} />
          </span>
        </div>
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-2 text-sm text-gray-600 cursor-pointer">
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
              className="w-4 h-4 rounded border-gray-300 text-[#62ac4a] focus:ring-[#62ac4a]"
            />
            Auto-refresh
          </label>
          <button
            onClick={fetchData}
            className="inline-flex items-center gap-2 px-3 py-1.5 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 transition"
          >
            <RefreshCw className="w-4 h-4" />
            Refresh
          </button>
        </div>
      </div>

      {/* Metric Cards */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4 mb-8">
        <MetricCard 
          title="Total Tool Calls" 
          value={totalCalls} 
          icon={Activity}
          color="#3b82f6"
        />
        <MetricCard 
          title="Errors" 
          value={totalErrors} 
          icon={XCircle}
          color="#ef4444"
        />
        <MetricCard 
          title="Blocked" 
          value={totalBlocked} 
          icon={Shield}
          color="#f59e0b"
        />
        <MetricCard 
          title="Active Sessions" 
          value={uniqueSessions} 
          icon={Globe}
          color="#8b5cf6"
        />
      </div>

      {/* Firewall Configuration */}
      <Section title="Firewall Configuration" icon={Shield}>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          <ConfigItem label="Default Policy" value={status.firewall.defaultPolicy} />
          <ConfigItem label="Tools Configured" value={status.firewall.toolsConfigured.toString()} />
          <ConfigItem label="Blocked Sessions" value={status.firewall.blockedSessions.toString()} />
          <ConfigItem 
            label="Data Leak Prevention" 
            value={status.firewall.dataLeakPrevention ? "Enabled" : "Disabled"}
            status={status.firewall.dataLeakPrevention ? "success" : "error"}
          />
          <ConfigItem 
            label="Lethal Trifecta Protection" 
            value={status.firewall.lethalTrifectaProtection ? "Enabled" : "Disabled"}
            status={status.firewall.lethalTrifectaProtection ? "success" : "error"}
          />
        </div>
      </Section>

      {/* Active Sessions */}
      <Section title="Active Sessions" icon={Globe}>
        {status.allSessions.length === 0 ? (
          <div className="text-center py-12 text-gray-500">
            <Globe className="w-12 h-12 mx-auto mb-3 text-gray-300" />
            <p>No active sessions</p>
          </div>
        ) : (
          <div className="overflow-x-auto -mx-6">
            <table className="w-full min-w-[700px]">
              <thead>
                <tr className="border-b border-gray-200">
                  <th className="text-left py-3 px-6 text-xs font-semibold text-gray-500 uppercase tracking-wider">Session ID</th>
                  <th className="text-left py-3 px-4 text-xs font-semibold text-gray-500 uppercase tracking-wider">Started</th>
                  <th className="text-left py-3 px-4 text-xs font-semibold text-gray-500 uppercase tracking-wider">Calls</th>
                  <th className="text-left py-3 px-4 text-xs font-semibold text-gray-500 uppercase tracking-wider">Errors</th>
                  <th className="text-left py-3 px-4 text-xs font-semibold text-gray-500 uppercase tracking-wider">Blocked</th>
                  <th className="text-left py-3 px-4 text-xs font-semibold text-gray-500 uppercase tracking-wider">ACL</th>
                  <th className="text-left py-3 px-4 text-xs font-semibold text-gray-500 uppercase tracking-wider">Risk</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-100">
                {status.allSessions.map((session) => (
                  <tr key={session.sessionId} className="hover:bg-gray-50 transition-colors">
                    <td className="py-3 px-6">
                      <code className="text-xs font-mono text-gray-600 bg-gray-100 px-2 py-1 rounded">
                        {session.sessionId.substring(0, 8)}...
                      </code>
                    </td>
                    <td className="py-3 px-4 text-sm text-gray-600">
                      {new Date(session.startTime).toLocaleTimeString()}
                    </td>
                    <td className="py-3 px-4 text-sm font-medium text-gray-900">
                      {session.toolCalls}
                    </td>
                    <td className="py-3 px-4">
                      <span className={`text-sm font-medium ${session.errors > 0 ? "text-red-600" : "text-gray-600"}`}>
                        {session.errors}
                      </span>
                    </td>
                    <td className="py-3 px-4">
                      <span className={`text-sm font-medium ${session.blocked > 0 ? "text-amber-600" : "text-gray-600"}`}>
                        {session.blocked}
                      </span>
                    </td>
                    <td className="py-3 px-4">
                      <AclBadge level={session.maxAclLevel} />
                    </td>
                    <td className="py-3 px-4">
                      {session.lethalTrifecta ? (
                        <span className="inline-flex items-center gap-1 text-red-600 font-semibold text-sm">
                          <AlertOctagon className="w-4 h-4" />
                          HIGH
                        </span>
                      ) : (
                        <span className="inline-flex items-center gap-1 text-green-600 font-medium text-sm">
                          <CheckCircle className="w-4 h-4" />
                          Normal
                        </span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Section>

      {/* Recent Audit Logs */}
      <Section title="Recent Activity" icon={Activity}>
        {status.recentLogs.length === 0 ? (
          <div className="text-center py-12 text-gray-500">
            <Activity className="w-12 h-12 mx-auto mb-3 text-gray-300" />
            <p>No recent activity</p>
          </div>
        ) : (
          <div className="space-y-3">
            {status.recentLogs.slice(0, 20).map((log) => (
              <div
                key={log.id}
                className={`p-4 rounded-xl border ${
                  log.result === "blocked" 
                    ? "bg-red-50 border-red-200" 
                    : log.result === "error" 
                    ? "bg-amber-50 border-amber-200" 
                    : "bg-green-50 border-green-200"
                }`}
              >
                <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2 mb-3">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-semibold text-gray-900">{log.toolName}</span>
                    <ResultBadge result={log.result} />
                    <AclBadge level={log.aclLevel} />
                  </div>
                  <span className="text-xs text-gray-500 flex items-center gap-1">
                    <Clock className="w-3 h-3" />
                    {new Date(log.timestamp).toLocaleString()}
                  </span>
                </div>
                <div className="flex flex-wrap items-center gap-4 text-sm text-gray-600">
                  <span className="font-mono text-xs bg-white px-2 py-1 rounded border">
                    {log.sessionId.substring(0, 8)}...
                  </span>
                  <span className="flex items-center gap-1">
                    <Clock className="w-3 h-3" />
                    {log.durationMs}ms
                  </span>
                  {log.riskFlags.writeOperation && (
                    <span className="inline-flex items-center gap-1 text-amber-600">
                      <Edit3 className="w-3 h-3" />
                      Write
                    </span>
                  )}
                  {log.riskFlags.readPrivateData && (
                    <span className="inline-flex items-center gap-1 text-amber-600">
                      <Lock className="w-3 h-3" />
                      Private Data
                    </span>
                  )}
                  {log.riskFlags.externalCommunication && (
                    <span className="inline-flex items-center gap-1 text-purple-600">
                      <Globe className="w-3 h-3" />
                      External
                    </span>
                  )}
                </div>
                {log.error && (
                  <div className="mt-3 p-3 bg-red-100 rounded-lg text-sm text-red-800">
                    <span className="font-semibold">Error:</span> {log.error}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </Section>

      {/* Alerts */}
      {status.alerts.length > 0 && (
        <Section title="Security Alerts" icon={AlertTriangle}>
          <div className="space-y-3">
            {status.alerts.map((alert, index) => (
              <div
                key={index}
                className="flex items-start gap-3 p-4 bg-red-50 border border-red-200 rounded-xl"
              >
                <AlertTriangle className="w-5 h-5 text-red-600 flex-shrink-0 mt-0.5" />
                <p className="text-red-800 text-sm">{alert}</p>
              </div>
            ))}
          </div>
        </Section>
      )}
    </AdminLayout>
  );
}

function MetricCard({ 
  title, 
  value, 
  icon: Icon,
  color 
}: { 
  title: string; 
  value: number; 
  icon: React.ComponentType<{ className?: string }>;
  color: string;
}) {
  return (
    <div className="bg-white rounded-xl border border-gray-200 p-5 hover:shadow-md transition-shadow">
      <div className="flex items-center justify-between mb-3">
        <p className="text-sm font-medium text-gray-600">{title}</p>
        <div 
          className="w-10 h-10 rounded-lg flex items-center justify-center"
          style={{ backgroundColor: `${color}15` }}
        >
          <div style={{ color }}><Icon className="w-5 h-5" /></div>
        </div>
      </div>
      <p className="text-3xl font-bold text-gray-900">{value.toLocaleString()}</p>
    </div>
  );
}

function Section({ 
  title, 
  icon: Icon,
  children 
}: { 
  title: string; 
  icon: React.ComponentType<{ className?: string }>;
  children: React.ReactNode;
}) {
  return (
    <div className="mb-8 bg-white rounded-xl border border-gray-200 overflow-hidden">
      <div className="flex items-center gap-3 px-6 py-4 border-b border-gray-200 bg-gray-50/50">
        <Icon className="w-5 h-5 text-[#62ac4a]" />
        <h3 className="text-lg font-semibold text-gray-900">{title}</h3>
      </div>
      <div className="p-6">
        {children}
      </div>
    </div>
  );
}

function ConfigItem({ 
  label, 
  value,
  status
}: { 
  label: string; 
  value: string;
  status?: "success" | "error";
}) {
  return (
    <div className="flex items-center justify-between p-3 bg-gray-50 rounded-lg">
      <span className="text-sm text-gray-600">{label}</span>
      <span className={`text-sm font-semibold ${
        status === "success" ? "text-green-600" : 
        status === "error" ? "text-red-600" : 
        "text-gray-900"
      }`}>
        {value}
      </span>
    </div>
  );
}

function RiskBadge({ level }: { level: "low" | "medium" | "high" | "critical" }) {
  const styles = {
    low: "bg-green-100 text-green-800 border-green-200",
    medium: "bg-amber-100 text-amber-800 border-amber-200",
    high: "bg-orange-100 text-orange-800 border-orange-200",
    critical: "bg-red-100 text-red-800 border-red-200",
  };

  return (
    <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-semibold border ${styles[level]}`}>
      {level}
    </span>
  );
}

function AclBadge({ level }: { level: "PUBLIC" | "PRIVATE" | "SECRET" }) {
  const styles = {
    PUBLIC: "bg-green-100 text-green-800 border-green-200",
    PRIVATE: "bg-amber-100 text-amber-800 border-amber-200",
    SECRET: "bg-red-100 text-red-800 border-red-200",
  };

  return (
    <span className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold border ${styles[level]}`}>
      {level}
    </span>
  );
}

function ResultBadge({ result }: { result: "success" | "error" | "blocked" }) {
  const styles = {
    success: "bg-green-100 text-green-800 border-green-200",
    error: "bg-red-100 text-red-800 border-red-200",
    blocked: "bg-amber-100 text-amber-800 border-amber-200",
  };

  return (
    <span className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold border ${styles[result]}`}>
      {result}
    </span>
  );
}
