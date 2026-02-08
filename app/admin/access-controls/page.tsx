"use client";

import { useEffect, useState } from "react";
import AdminLayout from "../components/AdminLayout";
import { 
  Users, 
  UserCheck, 
  UserX, 
  Mail, 
  Shield, 
  Edit2, 
  X,
  Loader2,
  CheckCircle,
  AlertCircle,
  Plus,
  Search
} from "lucide-react";

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
  const [error, setError] = useState<string | null>(null);
  const [selectedUser, setSelectedUser] = useState<User | null>(null);
  const [showRoleModal, setShowRoleModal] = useState(false);
  const [updatingRole, setUpdatingRole] = useState(false);
  const [successMessage, setSuccessMessage] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  useEffect(() => {
    fetchUsers();
  }, []);

  useEffect(() => {
    if (successMessage) {
      const timer = setTimeout(() => setSuccessMessage(null), 3000);
      return () => clearTimeout(timer);
    }
  }, [successMessage]);

  async function fetchUsers() {
    setLoading(true);
    setError(null);
    try {
      const response = await fetch("/api/users");
      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to fetch users");
      }
      const data = await response.json();
      setUsers(data.users || []);
    } catch (err) {
      console.error("[AccessControls] Error fetching users:", err);
      setError(err instanceof Error ? err.message : "Failed to fetch users");
    } finally {
      setLoading(false);
    }
  }

  async function updateUserRole(userId: string, newRole: string) {
    setUpdatingRole(true);
    setError(null);
    try {
      const response = await fetch("/api/users", {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ userId, role: newRole }),
      });

      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error || "Failed to update role");
      }

      const data = await response.json();

      setUsers(users.map((u) => (u.id === userId ? data.user : u)));
      setSuccessMessage(`Role updated successfully for ${selectedUser?.name || selectedUser?.email}`);
      setShowRoleModal(false);
      setSelectedUser(null);
    } catch (err) {
      console.error("[AccessControls] Error updating role:", err);
      setError(err instanceof Error ? err.message : "Failed to update role");
    } finally {
      setUpdatingRole(false);
    }
  }

  const filteredUsers = users.filter(user => 
    user.email.toLowerCase().includes(searchQuery.toLowerCase()) ||
    (user.name && user.name.toLowerCase().includes(searchQuery.toLowerCase()))
  );

  if (loading) {
    return (
      <AdminLayout title="Access Controls">
        <div className="flex items-center justify-center min-h-[400px]">
          <div className="flex items-center gap-3 text-gray-500">
            <Loader2 className="w-6 h-6 animate-spin text-[#62ac4a]" />
            <span>Loading users...</span>
          </div>
        </div>
      </AdminLayout>
    );
  }

  return (
    <AdminLayout title="Access Controls">
      {/* Success Message */}
      {successMessage && (
        <div className="mb-6 flex items-center gap-2 p-4 bg-green-50 border border-green-200 rounded-xl text-green-800">
          <CheckCircle className="w-5 h-5 flex-shrink-0" />
          <span>{successMessage}</span>
        </div>
      )}

      {/* Error Message */}
      {error && (
        <div className="mb-6 flex items-center justify-between gap-2 p-4 bg-red-50 border border-red-200 rounded-xl text-red-800">
          <div className="flex items-center gap-2">
            <AlertCircle className="w-5 h-5 flex-shrink-0" />
            <span>{error}</span>
          </div>
          <button
            onClick={fetchUsers}
            className="px-3 py-1.5 text-sm font-medium bg-white border border-red-200 rounded-lg hover:bg-red-50 transition"
          >
            Retry
          </button>
        </div>
      )}

      {/* Stats Overview */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4 mb-8">
        <StatCard 
          title="Total Users" 
          value={users.length} 
          icon={Users}
          color="#3b82f6"
        />
        <StatCard 
          title="Admins" 
          value={users.filter((u) => u.role === "admin").length} 
          icon={Shield}
          color="#8b5cf6"
        />
        <StatCard 
          title="Verified Emails" 
          value={users.filter((u) => u.emailVerified).length} 
          icon={UserCheck}
          color="#10b981"
        />
        <StatCard 
          title="Pending Verification" 
          value={users.filter((u) => !u.emailVerified).length} 
          icon={UserX}
          color="#f59e0b"
        />
      </div>

      {/* Users Section */}
      <div className="bg-white rounded-xl border border-gray-200 overflow-hidden mb-8">
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 p-6 border-b border-gray-200">
          <div>
            <h3 className="text-lg font-semibold text-gray-900">Users</h3>
            <p className="text-sm text-gray-500 mt-1">Manage user roles and permissions</p>
          </div>
          <div className="flex items-center gap-3">
            <div className="relative">
              <Search className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
              <input
                type="text"
                placeholder="Search users..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="pl-9 pr-4 py-2 text-sm border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-[#62ac4a] focus:border-transparent w-full sm:w-64"
              />
            </div>
            <button className="inline-flex items-center gap-2 px-4 py-2 bg-[#62ac4a] text-white rounded-lg hover:bg-[#4e8a3a] transition font-medium text-sm">
              <Plus className="w-4 h-4" />
              Invite User
            </button>
          </div>
        </div>

        {filteredUsers.length === 0 ? (
          <div className="text-center py-12 text-gray-500">
            <Users className="w-12 h-12 mx-auto mb-3 text-gray-300" />
            <p>No users found</p>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[700px]">
              <thead>
                <tr className="border-b border-gray-200 bg-gray-50/50">
                  <th className="text-left py-4 px-6 text-xs font-semibold text-gray-500 uppercase tracking-wider">User</th>
                  <th className="text-left py-4 px-4 text-xs font-semibold text-gray-500 uppercase tracking-wider">Role</th>
                  <th className="text-left py-4 px-4 text-xs font-semibold text-gray-500 uppercase tracking-wider">Status</th>
                  <th className="text-left py-4 px-4 text-xs font-semibold text-gray-500 uppercase tracking-wider">Joined</th>
                  <th className="text-left py-4 px-4 text-xs font-semibold text-gray-500 uppercase tracking-wider">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-100">
                {filteredUsers.map((user) => (
                  <tr key={user.id} className="hover:bg-gray-50 transition-colors">
                    <td className="py-4 px-6">
                      <div className="flex items-center gap-3">
                        <div className="w-10 h-10 rounded-full bg-gradient-to-br from-[#62ac4a] to-[#41734a] flex items-center justify-center text-white font-semibold flex-shrink-0">
                          {user.image ? (
                            <img 
                              src={user.image} 
                              alt={user.name || ""} 
                              className="w-full h-full object-cover rounded-full"
                            />
                          ) : (
                            (user.name || user.email).charAt(0).toUpperCase()
                          )}
                        </div>
                        <div className="min-w-0">
                          <div className="font-medium text-gray-900 truncate">
                            {user.name || "Unnamed User"}
                          </div>
                          <div className="text-sm text-gray-500 flex items-center gap-1">
                            <Mail className="w-3 h-3 flex-shrink-0" />
                            <span className="truncate">{user.email}</span>
                          </div>
                        </div>
                      </div>
                    </td>
                    <td className="py-4 px-4">
                      <RoleBadge role={user.role} />
                    </td>
                    <td className="py-4 px-4">
                      <StatusBadge verified={user.emailVerified} />
                    </td>
                    <td className="py-4 px-4 text-sm text-gray-600">
                      {new Date(user.createdAt).toLocaleDateString()}
                    </td>
                    <td className="py-4 px-4">
                      <button
                        onClick={() => {
                          setSelectedUser(user);
                          setShowRoleModal(true);
                        }}
                        className="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 hover:border-[#62ac4a] hover:text-[#41734a] transition"
                      >
                        <Edit2 className="w-3.5 h-3.5" />
                        Edit Role
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Roles Section */}
      <div className="bg-white rounded-xl border border-gray-200 overflow-hidden">
        <div className="p-6 border-b border-gray-200">
          <h3 className="text-lg font-semibold text-gray-900">Roles & Permissions</h3>
          <p className="text-sm text-gray-500 mt-1">Manage available roles and their permissions</p>
        </div>
        <div className="p-6">
          <div className="grid gap-4">
            {roles.map((role) => (
              <div
                key={role.id}
                className="p-5 bg-gray-50 rounded-xl border border-gray-200 hover:border-[#62ac4a]/30 transition-colors"
              >
                <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-3 mb-3">
                  <div className="flex items-center gap-3">
                    <RoleBadge role={role.id} />
                    <span className="text-sm text-gray-500">({role.permissions.length} permissions)</span>
                  </div>
                  <button className="self-start sm:self-auto px-3 py-1.5 text-xs font-medium text-gray-600 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 hover:border-[#62ac4a] transition">
                    Edit Permissions
                  </button>
                </div>
                <p className="text-sm text-gray-600 mb-3">{role.description}</p>
                <div className="flex flex-wrap gap-2">
                  {role.permissions.map((perm) => (
                    <span
                      key={perm}
                      className="px-2 py-1 bg-white border border-gray-200 rounded text-xs font-mono text-gray-600"
                    >
                      {perm}
                    </span>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Role Modal */}
      {showRoleModal && selectedUser && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50">
          <div className="bg-white rounded-2xl shadow-xl w-full max-w-md overflow-hidden">
            <div className="flex items-center justify-between p-6 border-b border-gray-200">
              <h3 className="text-lg font-semibold text-gray-900">
                Change Role for {selectedUser.name || selectedUser.email}
              </h3>
              <button
                onClick={() => {
                  setShowRoleModal(false);
                  setSelectedUser(null);
                }}
                disabled={updatingRole}
                className="p-2 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition"
              >
                <X className="w-5 h-5" />
              </button>
            </div>
            <div className="p-6 space-y-3">
              {roles.map((role) => (
                <button
                  key={role.id}
                  onClick={() => !updatingRole && updateUserRole(selectedUser.id, role.id)}
                  disabled={updatingRole}
                  className={`w-full flex items-center gap-4 p-4 rounded-xl border-2 text-left transition-all ${
                    selectedUser.role === role.id
                      ? "border-[#62ac4a] bg-[#62ac4a]/5"
                      : "border-gray-200 hover:border-[#62ac4a]/50 hover:bg-gray-50"
                  } ${updatingRole ? "opacity-60 cursor-not-allowed" : ""}`}
                >
                  <RoleBadge role={role.id} />
                  <div className="flex-1 min-w-0">
                    <div className="font-medium text-gray-900">{role.name}</div>
                    <div className="text-sm text-gray-500">{role.description}</div>
                  </div>
                  {selectedUser.role === role.id && (
                    <CheckCircle className="w-5 h-5 text-[#62ac4a] flex-shrink-0" />
                  )}
                  {updatingRole && selectedUser.role !== role.id && (
                    <Loader2 className="w-5 h-5 text-gray-400 animate-spin flex-shrink-0" />
                  )}
                </button>
              ))}
            </div>
            <div className="flex justify-end gap-3 p-6 border-t border-gray-200 bg-gray-50">
              <button
                onClick={() => {
                  setShowRoleModal(false);
                  setSelectedUser(null);
                }}
                disabled={updatingRole}
                className="px-4 py-2 text-sm font-medium text-gray-700 bg-white border border-gray-300 rounded-lg hover:bg-gray-50 transition disabled:opacity-50"
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}
    </AdminLayout>
  );
}

function LoadingSpinner({ size = "normal" }: { size?: "normal" | "small" }) {
  const sizeClass = size === "small" ? "w-4 h-4" : "w-6 h-6";
  return (
    <div className={`${sizeClass} border-2 border-gray-300 border-t-[#62ac4a] rounded-full animate-spin`} />
  );
}

function StatCard({ 
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
      <p className="text-3xl font-bold text-gray-900">{value}</p>
    </div>
  );
}

function RoleBadge({ role }: { role: string }) {
  const styles: Record<string, string> = {
    admin: "bg-purple-100 text-purple-800 border-purple-200",
    user: "bg-blue-100 text-blue-800 border-blue-200",
    viewer: "bg-gray-100 text-gray-800 border-gray-200",
  };

  const icons: Record<string, React.ComponentType<{ className?: string }>> = {
    admin: Shield,
    user: UserCheck,
    viewer: Users,
  };

  const Icon = icons[role] || Users;
  const style = styles[role] || styles.viewer;

  return (
    <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold border ${style}`}>
      <Icon className="w-3 h-3" />
      <span className="capitalize">{role}</span>
    </span>
  );
}

function StatusBadge({ verified }: { verified: boolean }) {
  return (
    <span className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold border ${
      verified 
        ? "bg-green-100 text-green-800 border-green-200" 
        : "bg-amber-100 text-amber-800 border-amber-200"
    }`}>
      {verified ? (
        <>
          <CheckCircle className="w-3 h-3" />
          Verified
        </>
      ) : (
        <>
          <AlertCircle className="w-3 h-3" />
          Pending
        </>
      )}
    </span>
  );
}
