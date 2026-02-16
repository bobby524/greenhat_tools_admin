"use client";

import { useEffect, useState } from "react";
import AdminLayout from "../components/AdminLayout";
import { 
  Users, 
  Search, 
  Mail,
  Shield,
  User,
  Users2,
  ChevronDown,
  ChevronUp
} from "lucide-react";

interface UserTeam {
  team_id: string;
  team_name: string;
  role: 'admin' | 'manager' | 'member';
}

interface UserData {
  id: string;
  first_name: string;
  last_name: string;
  email: string;
  avatar_url: string | null;
  org_role: string;
  joined_at: string;
  teams: UserTeam[];
}

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || "";

export default function UsersAdmin() {
  const [users, setUsers] = useState<UserData[]>([]);
  const [loading, setLoading] = useState(true);
  const [searchTerm, setSearchTerm] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [expandedUser, setExpandedUser] = useState<string | null>(null);

  useEffect(() => {
    loadUsers();
  }, []);

  async function loadUsers() {
    try {
      setLoading(true);
      const orgId = "cd861b76-f85c-4afc-b3e8-8f85945c3132"; // Default org
      const response = await fetch(`${API_BASE_URL}/api/admin/users?org_id=${orgId}`);
      if (!response.ok) throw new Error("Failed to load users");
      const data = await response.json();
      setUsers(data.users || []);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  const filteredUsers = users.filter(user =>
    user.first_name?.toLowerCase().includes(searchTerm.toLowerCase()) ||
    user.last_name?.toLowerCase().includes(searchTerm.toLowerCase()) ||
    user.email?.toLowerCase().includes(searchTerm.toLowerCase())
  );

  function getRoleBadge(role: string) {
    switch (role) {
      case 'admin':
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-purple-100 text-purple-700">
            <Shield className="w-3 h-3" />
            Admin
          </span>
        );
      case 'owner':
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-amber-100 text-amber-700">
            <Shield className="w-3 h-3" />
            Owner
          </span>
        );
      default:
        return (
          <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-gray-100 text-gray-700">
            <User className="w-3 h-3" />
            Member
          </span>
        );
    }
  }

  return (
    <AdminLayout title="User Management">
      {/* Header Actions */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-6">
        <div className="relative flex-1 max-w-md">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
          <input
            type="text"
            placeholder="Search users by name or email..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="w-full pl-10 pr-4 py-2 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-[#62ac4a] focus:border-transparent"
          />
        </div>
        <div className="text-sm text-gray-500">
          {users.length} total users
        </div>
      </div>

      {/* Error Alert */}
      {error && (
        <div className="mb-6 p-4 bg-red-50 border border-red-200 rounded-lg text-red-700">
          {error}
        </div>
      )}

      {/* Users List */}
      {loading ? (
        <div className="flex items-center justify-center py-12">
          <div className="w-8 h-8 border-2 border-gray-300 border-t-[#62ac4a] rounded-full animate-spin" />
        </div>
      ) : filteredUsers.length === 0 ? (
        <div className="text-center py-12 bg-gray-50 rounded-xl border border-gray-200">
          <Users className="w-12 h-12 text-gray-300 mx-auto mb-4" />
          <h3 className="text-lg font-medium text-gray-900 mb-1">No users found</h3>
          <p className="text-gray-500">
            {searchTerm ? "Try adjusting your search" : "No users in this organization yet"}
          </p>
        </div>
      ) : (
        <div className="bg-white rounded-xl border border-gray-200 overflow-hidden">
          <div className="divide-y divide-gray-100">
            {filteredUsers.map((user) => (
              <div key={user.id} className="group">
                <div
                  className="flex items-center justify-between p-4 hover:bg-gray-50 transition cursor-pointer"
                  onClick={() => setExpandedUser(expandedUser === user.id ? null : user.id)}
                >
                  <div className="flex items-center gap-4">
                    <div className="w-10 h-10 rounded-full bg-gradient-to-br from-[#62ac4a] to-[#41734a] flex items-center justify-center text-white font-medium">
                      {user.first_name?.[0]}{user.last_name?.[0]}
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <h3 className="font-semibold text-gray-900">
                          {user.first_name} {user.last_name}
                        </h3>
                        {getRoleBadge(user.org_role)}
                      </div>
                      <p className="text-sm text-gray-500">{user.email}</p>
                    </div>
                  </div>
                  <div className="flex items-center gap-4">
                    {user.teams.length > 0 && (
                      <div className="flex items-center gap-1.5 text-sm text-gray-500">
                        <Users2 className="w-4 h-4" />
                        <span>{user.teams.length} team{user.teams.length !== 1 ? 's' : ''}</span>
                      </div>
                    )}
                    {expandedUser === user.id ? (
                      <ChevronUp className="w-5 h-5 text-gray-400" />
                    ) : (
                      <ChevronDown className="w-5 h-5 text-gray-400" />
                    )}
                  </div>
                </div>

                {/* Expanded Details */}
                {expandedUser === user.id && (
                  <div className="px-4 pb-4 pl-18 bg-gray-50/50">
                    <div className="ml-14 space-y-4">
                      <div className="flex items-center gap-4 text-sm">
                        <div className="flex items-center gap-2 text-gray-600">
                          <Mail className="w-4 h-4" />
                          <span>{user.email}</span>
                        </div>
                        <div className="text-gray-400">
                          Joined {new Date(user.joined_at).toLocaleDateString()}
                        </div>
                      </div>

                      {/* Teams Section */}
                      <div>
                        <h4 className="text-sm font-medium text-gray-700 mb-2">Team Memberships</h4>
                        {user.teams.length === 0 ? (
                          <p className="text-sm text-gray-500">Not assigned to any teams</p>
                        ) : (
                          <div className="flex flex-wrap gap-2">
                            {user.teams.map((team) => (
                              <a
                                key={team.team_id}
                                href={`/admin/teams/${team.team_id}`}
                                className="inline-flex items-center gap-1.5 px-3 py-1.5 bg-white border border-gray-200 rounded-lg text-sm hover:border-[#62ac4a]/50 hover:shadow-sm transition"
                              >
                                <span className="font-medium text-gray-700">{team.team_name}</span>
                                <span className={`text-xs px-1.5 py-0.5 rounded ${
                                  team.role === 'lead' 
                                    ? 'bg-amber-100 text-amber-700' 
                                    : 'bg-gray-100 text-gray-600'
                                }`}>
                                  {team.role}
                                </span>
                              </a>
                            ))}
                          </div>
                        )}
                      </div>

                      {/* Actions */}
                      <div className="flex items-center gap-3 pt-2">
                        <a
                          href={`/admin/teams`}
                          className="inline-flex items-center gap-1.5 text-sm text-[#62ac4a] hover:underline"
                        >
                          <Users2 className="w-4 h-4" />
                          Manage in Teams
                        </a>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
    </AdminLayout>
  );
}
