"use client";

import { useEffect, useState } from "react";
import AdminLayout from "../components/AdminLayout";

interface User {
  id: string;
  email: string;
  name: string | null;
  image: string | null;
  role: string;
  emailVerified: boolean;
  createdAt: string;
  updatedAt: string;
}

interface Role {
  id: string;
  name: string;
  permissions: string[];
  description: string;
}

const DEFAULT_ROLES: Role[] = [
  {
    id: "admin",
    name: "Admin",
    permissions: ["*"],
    description: "Full access to all features",
  },
  {
    id: "user",
    name: "User",
    permissions: ["read:own", "write:own"],
    description: "Standard user access",
  },
  {
    id: "viewer",
    name: "Viewer",
    permissions: ["read:own"],
    description: "Read-only access",
  },
];

export default function AccessControlsModule() {
  const [users, setUsers] = useState<User[]>([]);
  const [roles, setRoles] = useState<Role[]>(DEFAULT_ROLES);
  const [loading, setLoading] = useState(true);
  const [selectedUser, setSelectedUser] = useState<User | null>(null);
  const [showRoleModal, setShowRoleModal] = useState(false);

  useEffect(() => {
    fetchUsers();
  }, []);

  async function fetchUsers() {
    // Mock data - in production this would call your API
    setUsers([
      {
        id: "09649c79-975a-4967-9299-440b2b0fadee",
        email: "anthony@greenhatsec.com",
        name: "Anthony",
        image: null,
        role: "admin",
        emailVerified: true,
        createdAt: "2026-01-15T10:00:00Z",
        updatedAt: "2026-02-07T12:00:00Z",
      },
      {
        id: "user-2",
        email: "bobby@greenhatsec.com",
        name: "Bobby",
        image: null,
        role: "admin",
        emailVerified: true,
        createdAt: "2026-01-20T14:30:00Z",
        updatedAt: "2026-02-06T09:15:00Z",
      },
      {
        id: "user-3",
        email: "demo@greenhatsec.com",
        name: "Demo User",
        image: null,
        role: "user",
        emailVerified: false,
        createdAt: "2026-02-01T08:00:00Z",
        updatedAt: "2026-02-01T08:00:00Z",
      },
    ]);
    setLoading(false);
  }

  async function updateUserRole(userId: string, newRole: string) {
    // In production, this would call your API
    setUsers(users.map((u) => (u.id === userId ? { ...u, role: newRole } : u)));
    setShowRoleModal(false);
    setSelectedUser(null);
  }

  if (loading) {
    return (
      <AdminLayout title="Access Controls">
        <div>Loading users...</div>
      </AdminLayout>
    );
  }

  return (
    <AdminLayout title="Access Controls">
      {/* Stats Overview */}
      <div style={{ display: "grid", gap: "20px", gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))", marginBottom: "30px" }}>
        <StatCard title="Total Users" value={users.length} color="#3b82f6" />
        <StatCard title="Admins" value={users.filter((u) => u.role === "admin").length} color="#8b5cf6" />
        <StatCard title="Verified Emails" value={users.filter((u) => u.emailVerified).length} color="#10b981" />
        <StatCard title="Pending Verification" value={users.filter((u) => !u.emailVerified).length} color="#f59e0b" />
      </div>

      {/* Users Table */}
      <div style={{ padding: "20px", background: "white", borderRadius: "8px", border: "1px solid #e5e7eb", marginBottom: "30px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "20px" }}>
          <h3 style={{ margin: 0 }}>Users</h3>
          <button
            style={{
              padding: "8px 16px",
              background: "#2563eb",
              color: "white",
              border: "none",
              borderRadius: "6px",
              cursor: "pointer",
              fontSize: "0.875rem",
            }}
          >
            + Invite User
          </button>
        </div>

        <table style={{ width: "100%", borderCollapse: "collapse" }}>
          <thead>
            <tr style={{ background: "#f9fafb", textAlign: "left" }}>
              <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>User</th>
              <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Role</th>
              <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Status</th>
              <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Joined</th>
              <th style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {users.map((user) => (
              <tr key={user.id}>
                <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                    <div
                      style={{
                        width: "40px",
                        height: "40px",
                        borderRadius: "50%",
                        background: "#e5e7eb",
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "center",
                        fontSize: "1.25rem",
                      }}
                    >
                      {user.image ? (
                        <img src={user.image} alt={user.name || ""} style={{ width: "100%", height: "100%", borderRadius: "50%" }} />
                      ) : (
                        "👤"
                      )}
                    </div>
                    <div>
                      <div style={{ fontWeight: "bold" }}>{user.name || "Unnamed User"}</div>
                      <div style={{ fontSize: "0.875rem", color: "#6b7280" }}>{user.email}</div>
                    </div>
                  </div>
                </td>
                <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>
                  <RoleBadge role={user.role} />
                </td>
                <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>
                  <StatusBadge verified={user.emailVerified} />
                </td>
                <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb", color: "#6b7280", fontSize: "0.875rem" }}>
                  {new Date(user.createdAt).toLocaleDateString()}
                </td>
                <td style={{ padding: "12px", borderBottom: "1px solid #e5e7eb" }}>
                  <button
                    onClick={() => {
                      setSelectedUser(user);
                      setShowRoleModal(true);
                    }}
                    style={{
                      padding: "6px 12px",
                      background: "#f3f4f6",
                      border: "1px solid #d1d5db",
                      borderRadius: "4px",
                      cursor: "pointer",
                      fontSize: "0.875rem",
                    }}
                  >
                    Edit Role
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {/* Roles Section */}
      <div style={{ padding: "20px", background: "white", borderRadius: "8px", border: "1px solid #e5e7eb" }}>
        <h3 style={{ margin: "0 0 20px" }}>Roles & Permissions</h3>
        <div style={{ display: "flex", flexDirection: "column", gap: "15px" }}>
          {roles.map((role) => (
            <div
              key={role.id}
              style={{
                padding: "16px",
                background: "#f9fafb",
                borderRadius: "8px",
                border: "1px solid #e5e7eb",
              }}
            >
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "8px" }}>
                <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                  <RoleBadge role={role.id} />
                  <span style={{ fontSize: "0.875rem", color: "#6b7280" }}>({role.permissions.length} permissions)</span>
                </div>
                <button
                  style={{
                    padding: "4px 12px",
                    background: "transparent",
                    border: "1px solid #d1d5db",
                    borderRadius: "4px",
                    cursor: "pointer",
                    fontSize: "0.75rem",
                  }}
                >
                  Edit Permissions
                </button>
              </div>
              <p style={{ margin: 0, color: "#4b5563", fontSize: "0.875rem" }}>{role.description}</p>
              <div style={{ marginTop: "10px", display: "flex", flexWrap: "wrap", gap: "5px" }}>
                {role.permissions.map((perm) => (
                  <span
                    key={perm}
                    style={{
                      padding: "2px 8px",
                      background: "#e5e7eb",
                      borderRadius: "4px",
                      fontSize: "0.75rem",
                      color: "#4b5563",
                      fontFamily: "monospace",
                    }}
                  >
                    {perm}
                  </span>
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Role Modal */}
      {showRoleModal && selectedUser && (
        <div
          style={{
            position: "fixed",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            background: "rgba(0,0,0,0.5)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            zIndex: 1000,
          }}
        >
          <div style={{ background: "white", padding: "30px", borderRadius: "12px", minWidth: "400px", maxWidth: "500px" }}>
            <h3 style={{ margin: "0 0 20px" }}>Change Role for {selectedUser.name || selectedUser.email}</h3>
            <div style={{ display: "flex", flexDirection: "column", gap: "10px", marginBottom: "20px" }}>
              {roles.map((role) => (
                <button
                  key={role.id}
                  onClick={() => updateUserRole(selectedUser.id, role.id)}
                  style={{
                    padding: "12px 16px",
                    background: selectedUser.role === role.id ? "#eff6ff" : "white",
                    border: selectedUser.role === role.id ? "2px solid #3b82f6" : "1px solid #e5e7eb",
                    borderRadius: "8px",
                    cursor: "pointer",
                    textAlign: "left",
                    display: "flex",
                    alignItems: "center",
                    gap: "10px",
                  }}
                >
                  <RoleBadge role={role.id} />
                  <div>
                    <div style={{ fontWeight: "bold" }}>{role.name}</div>
                    <div style={{ fontSize: "0.875rem", color: "#6b7280" }}>{role.description}</div>
                  </div>
                </button>
              ))}
            </div>
            <div style={{ display: "flex", gap: "10px", justifyContent: "flex-end" }}>
              <button
                onClick={() => {
                  setShowRoleModal(false);
                  setSelectedUser(null);
                }}
                style={{
                  padding: "10px 20px",
                  background: "#f3f4f6",
                  border: "none",
                  borderRadius: "6px",
                  cursor: "pointer",
                }}
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Better Auth Dashboard Link */}
      <div
        style={{
          marginTop: "30px",
          padding: "20px",
          background: "#eff6ff",
          borderRadius: "8px",
          border: "1px solid #bfdbfe",
        }}
      >
        <h4 style={{ margin: "0 0 10px" }}>🔐 Better Auth Dashboard</h4>
        <p style={{ margin: "0 0 10px", color: "#4b5563" }}>
          This module is built on top of Better Auth. View the reference dashboard for more advanced features.
        </p>
        <a
          href="https://github.com/better-auth-extended/dashboard"
          target="_blank"
          rel="noopener noreferrer"
          style={{ color: "#2563eb" }}
        >
          View Better Auth Dashboard Reference →
        </a>
      </div>
    </AdminLayout>
  );
}

function StatCard({ title, value, color }: { title: string; value: number; color: string }) {
  return (
    <div style={{ padding: "20px", background: "white", borderRadius: "8px", border: "1px solid #e5e7eb" }}>
      <p style={{ margin: "0 0 8px", color: "#6b7280", fontSize: "0.875rem" }}>{title}</p>
      <p style={{ margin: 0, fontSize: "2rem", fontWeight: "bold", color }}>{value}</p>
    </div>
  );
}

function RoleBadge({ role }: { role: string }) {
  const colors: Record<string, { bg: string; color: string }> = {
    admin: { bg: "#fee2e2", color: "#991b1b" },
    user: { bg: "#dbeafe", color: "#1e40af" },
    viewer: { bg: "#f3f4f6", color: "#4b5563" },
  };

  const { bg, color } = colors[role] || colors.viewer;

  return (
    <span
      style={{
        display: "inline-block",
        padding: "4px 12px",
        borderRadius: "12px",
        fontSize: "0.75rem",
        fontWeight: "bold",
        textTransform: "capitalize",
        background: bg,
        color: color,
      }}
    >
      {role}
    </span>
  );
}

function StatusBadge({ verified }: { verified: boolean }) {
  return (
    <span
      style={{
        display: "inline-block",
        padding: "4px 12px",
        borderRadius: "12px",
        fontSize: "0.75rem",
        fontWeight: "bold",
        background: verified ? "#dcfce7" : "#fef3c7",
        color: verified ? "#166534" : "#92400e",
      }}
    >
      {verified ? "✓ Verified" : "○ Pending"}
    </span>
  );
}
