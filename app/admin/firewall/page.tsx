"use client";

import { useState } from "react";

interface ToolPermission {
  enabled: boolean;
  writeOperation: boolean;
  readPrivateData: boolean;
  readUntrustedPublicData: boolean;
  externalCommunication: boolean;
  acl: "SECRET";
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

const defaultToolPermissions: Record<string, ToolPermission> = {
  // CRM Admin Tools
  crm_list_all_customers: {
    enabled: true,
    writeOperation: false,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: "SECRET",
    rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
  },
  crm_delete_customer: {
    enabled: true,
    writeOperation: true,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: "SECRET",
    rateLimit: { requestsPerMinute: 10, requestsPerHour: 100 },
  },
  crm_export_customers: {
    enabled: true,
    writeOperation: false,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: "SECRET",
    rateLimit: { requestsPerMinute: 5, requestsPerHour: 50 },
  },

  // Exponential Admin Tools
  admin_list_all_users: {
    enabled: true,
    writeOperation: false,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: "SECRET",
    rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
  },
  admin_delete_user: {
    enabled: true,
    writeOperation: true,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: "SECRET",
    rateLimit: { requestsPerMinute: 5, requestsPerHour: 50 },
  },
  admin_get_audit_logs: {
    enabled: true,
    writeOperation: false,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: "SECRET",
    rateLimit: { requestsPerMinute: 30, requestsPerHour: 500 },
  },

  // System Admin Tools
  system_export_database: {
    enabled: true,
    writeOperation: false,
    readPrivateData: true,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: "SECRET",
    rateLimit: { requestsPerMinute: 2, requestsPerHour: 20 },
  },
  system_health_check: {
    enabled: true,
    writeOperation: false,
    readPrivateData: false,
    readUntrustedPublicData: false,
    externalCommunication: false,
    acl: "SECRET",
    rateLimit: { requestsPerMinute: 60, requestsPerHour: 1000 },
  },
};

export default function FirewallAdmin() {
  const [config, setConfig] = useState<FirewallConfig>({
    defaultPolicy: "deny",
    enableRateLimiting: true,
    enableDataLeakPrevention: true,
    enableLethalTrifectaProtection: true,
    toolPermissions: defaultToolPermissions,
    blockedPatterns: [
      "drop table",
      "delete from",
      "rm -rf",
      "exec(",
      "eval(",
    ],
  });

  const [selectedTool, setSelectedTool] = useState<string | null>(null);

  const toggleTool = (toolName: string) => {
    setConfig((prev) => ({
      ...prev,
      toolPermissions: {
        ...prev.toolPermissions,
        [toolName]: {
          ...prev.toolPermissions[toolName],
          enabled: !prev.toolPermissions[toolName].enabled,
        },
      },
    }));
  };

  const updateRateLimit = (
    toolName: string,
    field: "requestsPerMinute" | "requestsPerHour",
    value: number
  ) => {
    setConfig((prev) => {
      const currentTool = prev.toolPermissions[toolName];
      const currentRateLimit = currentTool.rateLimit || { requestsPerMinute: 0, requestsPerHour: 0 };
      return {
        ...prev,
        toolPermissions: {
          ...prev.toolPermissions,
          [toolName]: {
            ...currentTool,
            rateLimit: {
              ...currentRateLimit,
              [field]: value,
            },
          },
        },
      };
    });
  };

  const groupedTools = Object.entries(config.toolPermissions).reduce(
    (acc, [name, permission]) => {
      const category = name.split("_")[0];
      if (!acc[category]) acc[category] = [];
      acc[category].push({ name, permission });
      return acc;
    },
    {} as Record<string, { name: string; permission: ToolPermission }[]>
  );

  return (
    <div style={{ padding: "40px" }}>
      <h1>🛡️ MCP Firewall</h1>
      <p>Manage tool permissions and security policies for the Admin MCP</p>

      {/* Global Settings */}
      <div
        style={{
          marginTop: "40px",
          padding: "24px",
          background: "#f5f5f5",
          borderRadius: "8px",
        }}
      >
        <h2>Global Settings</h2>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(250px, 1fr))",
            gap: "16px",
            marginTop: "16px",
          }}
        >
          <label
            style={{
              display: "flex",
              alignItems: "center",
              gap: "8px",
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={config.enableRateLimiting}
              onChange={(e) =>
                setConfig((prev) => ({
                  ...prev,
                  enableRateLimiting: e.target.checked,
                }))
              }
            />
            Enable Rate Limiting
          </label>
          <label
            style={{
              display: "flex",
              alignItems: "center",
              gap: "8px",
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={config.enableDataLeakPrevention}
              onChange={(e) =>
                setConfig((prev) => ({
                  ...prev,
                  enableDataLeakPrevention: e.target.checked,
                }))
              }
            />
            Enable Data Leak Prevention
          </label>
          <label
            style={{
              display: "flex",
              alignItems: "center",
              gap: "8px",
              cursor: "pointer",
            }}
          >
            <input
              type="checkbox"
              checked={config.enableLethalTrifectaProtection}
              onChange={(e) =>
                setConfig((prev) => ({
                  ...prev,
                  enableLethalTrifectaProtection: e.target.checked,
                }))
              }
            />
            Enable Lethal Trifecta Protection
          </label>
        </div>
      </div>

      {/* Blocked Patterns */}
      <div style={{ marginTop: "32px" }}>
        <h2>Blocked Patterns</h2>
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: "8px",
            marginTop: "16px",
          }}
        >
          {config.blockedPatterns.map((pattern) => (
            <span
              key={pattern}
              style={{
                padding: "4px 12px",
                background: "#fee2e2",
                color: "#dc2626",
                borderRadius: "4px",
                fontSize: "14px",
                fontFamily: "monospace",
              }}
            >
              {pattern}
            </span>
          ))}
        </div>
      </div>

      {/* Tool Permissions */}
      <div style={{ marginTop: "32px" }}>
        <h2>Tool Permissions</h2>

        {Object.entries(groupedTools).map(([category, tools]) => (
          <div key={category} style={{ marginTop: "24px" }}>
            <h3 style={{ textTransform: "capitalize" }}>{category} Tools</h3>
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(300px, 1fr))",
                gap: "16px",
                marginTop: "16px",
              }}
            >
              {tools.map(({ name, permission }) => (
                <div
                  key={name}
                  style={{
                    padding: "16px",
                    border: "1px solid #ddd",
                    borderRadius: "8px",
                    background: permission.enabled ? "white" : "#f5f5f5",
                  }}
                >
                  <div
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      alignItems: "center",
                    }}
                  >
                    <h4 style={{ margin: 0, fontSize: "14px" }}>{name}</h4>
                    <label
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: "8px",
                        cursor: "pointer",
                      }}
                    >
                      <input
                        type="checkbox"
                        checked={permission.enabled}
                        onChange={() => toggleTool(name)}
                      />
                      Enabled
                    </label>
                  </div>

                  <div
                    style={{
                      marginTop: "12px",
                      display: "flex",
                      flexWrap: "wrap",
                      gap: "8px",
                    }}
                  >
                    {permission.writeOperation && (
                      <span
                        style={{
                          padding: "2px 8px",
                          background: "#fef3c7",
                          color: "#d97706",
                          borderRadius: "4px",
                          fontSize: "12px",
                        }}
                      >
                        Write
                      </span>
                    )}
                    {permission.readPrivateData && (
                      <span
                        style={{
                          padding: "2px 8px",
                          background: "#dbeafe",
                          color: "#2563eb",
                          borderRadius: "4px",
                          fontSize: "12px",
                        }}
                      >
                        Private Data
                      </span>
                    )}
                    {permission.externalCommunication && (
                      <span
                        style={{
                          padding: "2px 8px",
                          background: "#fce7f3",
                          color: "#db2777",
                          borderRadius: "4px",
                          fontSize: "12px",
                        }}
                      >
                        External
                      </span>
                    )}
                    <span
                      style={{
                        padding: "2px 8px",
                        background: "#dc2626",
                        color: "white",
                        borderRadius: "4px",
                        fontSize: "12px",
                      }}
                    >
                      {permission.acl}
                    </span>
                  </div>

                  {permission.rateLimit && (
                    <div
                      style={{
                        marginTop: "12px",
                        padding: "8px",
                        background: "#f5f5f5",
                        borderRadius: "4px",
                        fontSize: "12px",
                      }}
                    >
                      <div>Rate Limits:</div>
                      <div style={{ marginTop: "4px" }}>
                        {permission.rateLimit.requestsPerMinute}/min,{" "}
                        {permission.rateLimit.requestsPerHour}/hour
                      </div>
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>

      <div style={{ marginTop: "40px" }}>
        <a href="/admin">← Back to Admin Dashboard</a>
      </div>
    </div>
  );
}
