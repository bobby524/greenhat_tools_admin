"use client";

import { useEffect, useState } from "react";
import { useParams } from "next/navigation";
import AdminLayout from "../../components/AdminLayout";
import { 
  Users, 
  Plus, 
  Search, 
  ArrowLeft,
  UserPlus,
  X,
  Shield,
  User,
  FolderGit
} from "lucide-react";

interface TeamMember {
  id: string;
  user_id: string;
  first_name: string;
  last_name: string;
  email: string;
  avatar_url: string | null;
  role: 'admin' | 'manager' | 'member';
  created_at: string;
}

interface Team {
  id: string;
  name: string;
  slug: string;
  color: string;
  org_id: string;
  created_at: string;
}

interface Project {
  id: string;
  name: string;
  color: string;
  state: string;
  created_at: string;
}

interface User {
  id: string;
  first_name: string;
  last_name: string;
  email: string;
  avatar_url: string | null;
}

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || "";

export default function TeamDetailPage() {
  const params = useParams();
  const teamId = params.id as string;

  const [team, setTeam] = useState<Team | null>(null);
  const [members, setMembers] = useState<TeamMember[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Add member modal state
  const [showAddModal, setShowAddModal] = useState(false);
  const [availableUsers, setAvailableUsers] = useState<User[]>([]);
  const [userSearchTerm, setUserSearchTerm] = useState("");
  const [selectedUserId, setSelectedUserId] = useState<string>("");
  const [selectedRole, setSelectedRole] = useState<'admin' | 'manager' | 'member'>("member");
  const [addingMember, setAddingMember] = useState(false);

  useEffect(() => {
    if (teamId) {
      loadTeamDetails();
    }
  }, [teamId]);

  async function loadTeamDetails() {
    try {
      setLoading(true);
      const response = await fetch(`${API_BASE_URL}/api/admin/teams/${teamId}`);
      if (!response.ok) throw new Error("Failed to load team");
      const data = await response.json();
      setTeam(data.team);
      setMembers(data.members || []);
      setProjects(data.projects || []);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  async function loadAvailableUsers() {
    try {
      const orgId = "cd861b76-f85c-4afc-b3e8-8f85945c3132"; // Default org
      const response = await fetch(
        `${API_BASE_URL}/api/admin/users?org_id=${orgId}&not_in_team=${teamId}`
      );
      if (!response.ok) throw new Error("Failed to load users");
      const data = await response.json();
      setAvailableUsers(data.users || []);
    } catch (err: any) {
      setError(err.message);
    }
  }

  async function addMember(e: React.FormEvent) {
    e.preventDefault();
    if (!selectedUserId) return;

    try {
      setAddingMember(true);
      const response = await fetch(`${API_BASE_URL}/api/admin/team-members`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          team_id: teamId,
          user_id: selectedUserId,
          role: selectedRole,
        }),
      });
      if (!response.ok) throw new Error("Failed to add member");
      setShowAddModal(false);
      setSelectedUserId("");
      setSelectedRole("member");
      loadTeamDetails();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setAddingMember(false);
    }
  }

  async function removeMember(memberId: string) {
    if (!confirm("Are you sure you want to remove this member from the team?")) return;
    try {
      const response = await fetch(
        `${API_BASE_URL}/api/admin/team-members?id=${memberId}`,
        { method: "DELETE" }
      );
      if (!response.ok) throw new Error("Failed to remove member");
      loadTeamDetails();
    } catch (err: any) {
      setError(err.message);
    }
  }

  async function updateMemberRole(memberId: string, newRole: 'lead' | 'member') {
    try {
      const response = await fetch(`${API_BASE_URL}/api/admin/team-members`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          id: memberId,
          role: newRole,
        }),
      });
      if (!response.ok) throw new Error("Failed to update member role");
      loadTeamDetails();
    } catch (err: any) {
      setError(err.message);
    }
  }

  function openAddModal() {
    loadAvailableUsers();
    setShowAddModal(true);
  }

  const filteredUsers = availableUsers.filter(user =>
    user.first_name?.toLowerCase().includes(userSearchTerm.toLowerCase()) ||
    user.last_name?.toLowerCase().includes(userSearchTerm.toLowerCase()) ||
    user.email?.toLowerCase().includes(userSearchTerm.toLowerCase())
  );

  if (loading) {
    return (
      <AdminLayout title="Team Details">
        <div className="flex items-center justify-center py-12">
          <div className="w-8 h-8 border-2 border-gray-300 border-t-[#62ac4a] rounded-full animate-spin" />
        </div>
      </AdminLayout>
    );
  }

  if (!team) {
    return (
      <AdminLayout title="Team Not Found">
        <div className="text-center py-12">
          <h2 className="text-xl font-semibold text-gray-900 mb-2">Team not found</h2>
          <a href="/admin/teams" className="text-[#62ac4a] hover:underline">
            Back to teams
          </a>
        </div>
      </AdminLayout>
    );
  }

  return (
    <AdminLayout title={team.name}>
      {/* Breadcrumb */}
      <div className="mb-6">
        <a
          href="/admin/teams"
          className="inline-flex items-center gap-2 text-gray-500 hover:text-[#62ac4a] transition"
        >
          <ArrowLeft className="w-4 h-4" />
          Back to Teams
        </a>
      </div>

      {/* Error Alert */}
      {error && (
        <div className="mb-6 p-4 bg-red-50 border border-red-200 rounded-lg text-red-700">
          {error}
        </div>
      )}

      {/* Team Header */}
      <div className="bg-white rounded-xl border border-gray-200 p-6 mb-6">
        <div className="flex items-center gap-4">
          <div
            className="w-16 h-16 rounded-xl flex items-center justify-center"
            style={{ backgroundColor: `${team.color}15` }}
          >
            <Users className="w-8 h-8" style={{ color: team.color }} />
          </div>
          <div className="flex-1">
            <h1 className="text-2xl font-semibold text-gray-900">{team.name}</h1>
            <p className="text-gray-500">@{team.slug}</p>
          </div>
          <div className="flex items-center gap-4 text-sm text-gray-600">
            <div className="flex items-center gap-1.5 px-3 py-1.5 bg-gray-50 rounded-lg">
              <Users className="w-4 h-4 text-gray-400" />
              <span>{members.length} members</span>
            </div>
            <div className="flex items-center gap-1.5 px-3 py-1.5 bg-gray-50 rounded-lg">
              <FolderGit className="w-4 h-4 text-gray-400" />
              <span>{projects.length} projects</span>
            </div>
          </div>
        </div>
      </div>

      <div className="grid lg:grid-cols-3 gap-6">
        {/* Members Section */}
        <div className="lg:col-span-2 space-y-6">
          <div className="bg-white rounded-xl border border-gray-200">
            <div className="flex items-center justify-between p-6 border-b border-gray-200">
              <h2 className="text-lg font-semibold text-gray-900">Team Members</h2>
              <button
                onClick={openAddModal}
                className="inline-flex items-center gap-2 px-4 py-2 bg-[#62ac4a] text-white rounded-lg hover:bg-[#4e8a3a] transition font-medium text-sm"
              >
                <UserPlus className="w-4 h-4" />
                Add Member
              </button>
            </div>

            {members.length === 0 ? (
              <div className="p-8 text-center">
                <Users className="w-12 h-12 text-gray-300 mx-auto mb-4" />
                <h3 className="text-lg font-medium text-gray-900 mb-1">No members yet</h3>
                <p className="text-gray-500 mb-4">Add team members to get started</p>
                <button
                  onClick={openAddModal}
                  className="inline-flex items-center gap-2 px-4 py-2 border border-[#62ac4a] text-[#62ac4a] rounded-lg hover:bg-[#62ac4a]/5 transition"
                >
                  <UserPlus className="w-4 h-4" />
                  Add First Member
                </button>
              </div>
            ) : (
              <div className="divide-y divide-gray-100">
                {members.map((member) => (
                  <div
                    key={member.id}
                    className="flex items-center justify-between p-4 hover:bg-gray-50 transition"
                  >
                    <div className="flex items-center gap-3">
                      <div className="w-10 h-10 rounded-full bg-gradient-to-br from-[#62ac4a] to-[#41734a] flex items-center justify-center text-white font-medium">
                        {member.first_name?.[0]}{member.last_name?.[0]}
                      </div>
                      <div>
                        <p className="font-medium text-gray-900">
                          {member.first_name} {member.last_name}
                        </p>
                        <p className="text-sm text-gray-500">{member.email}</p>
                      </div>
                    </div>
                    <div className="flex items-center gap-3">
                      <select
                        value={member.role}
                        onChange={(e) => updateMemberRole(member.id, e.target.value as 'admin' | 'manager' | 'member')}
                        className="text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-2 focus:ring-[#62ac4a]"
                      >
                        <option value="member">Member</option>
                        <option value="manager">Manager</option>
                        <option value="admin">Admin</option>
                      </select>
                      <button
                        onClick={() => removeMember(member.id)}
                        className="p-2 text-gray-400 hover:text-red-600 hover:bg-red-50 rounded-lg transition"
                        title="Remove from team"
                      >
                        <X className="w-4 h-4" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Projects Section */}
        <div>
          <div className="bg-white rounded-xl border border-gray-200">
            <div className="p-6 border-b border-gray-200">
              <h2 className="text-lg font-semibold text-gray-900">Projects</h2>
            </div>
            {projects.length === 0 ? (
              <div className="p-6 text-center text-gray-500">
                No projects in this team
              </div>
            ) : (
              <div className="divide-y divide-gray-100">
                {projects.map((project) => (
                  <div
                    key={project.id}
                    className="flex items-center gap-3 p-4 hover:bg-gray-50 transition"
                  >
                    <div
                      className="w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0"
                      style={{ backgroundColor: `${project.color}15` }}
                    >
                      <FolderGit className="w-4 h-4" style={{ color: project.color }} />
                    </div>
                    <div className="flex-1 min-w-0">
                      <p className="font-medium text-gray-900 truncate">{project.name}</p>
                      <p className="text-xs text-gray-500 capitalize">{project.state}</p>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Add Member Modal */}
      {showAddModal && (
        <div className="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4">
          <div className="bg-white rounded-xl shadow-xl max-w-lg w-full max-h-[80vh] flex flex-col">
            <div className="flex items-center justify-between p-6 border-b border-gray-200">
              <h2 className="text-lg font-semibold text-gray-900">Add Team Member</h2>
              <button
                onClick={() => setShowAddModal(false)}
                className="text-gray-400 hover:text-gray-600"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            <form onSubmit={addMember} className="flex-1 overflow-hidden flex flex-col">
              <div className="p-6 space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">
                    Search Users
                  </label>
                  <div className="relative">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
                    <input
                      type="text"
                      placeholder="Search by name or email..."
                      value={userSearchTerm}
                      onChange={(e) => setUserSearchTerm(e.target.value)}
                      className="w-full pl-10 pr-4 py-2 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-[#62ac4a]"
                    />
                  </div>
                </div>

                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">
                    Select User
                  </label>
                  <div className="max-h-48 overflow-y-auto border border-gray-200 rounded-lg divide-y divide-gray-100">
                    {filteredUsers.length === 0 ? (
                      <div className="p-4 text-center text-gray-500">
                        No available users found
                      </div>
                    ) : (
                      filteredUsers.map((user) => (
                        <label
                          key={user.id}
                          className={`flex items-center gap-3 p-3 cursor-pointer hover:bg-gray-50 transition ${
                            selectedUserId === user.id ? "bg-[#62ac4a]/5" : ""
                          }`}
                        >
                          <input
                            type="radio"
                            name="user"
                            value={user.id}
                            checked={selectedUserId === user.id}
                            onChange={(e) => setSelectedUserId(e.target.value)}
                            className="w-4 h-4 text-[#62ac4a] focus:ring-[#62ac4a]"
                          />
                          <div className="w-8 h-8 rounded-full bg-gradient-to-br from-[#62ac4a] to-[#41734a] flex items-center justify-center text-white text-sm font-medium">
                            {user.first_name?.[0]}{user.last_name?.[0]}
                          </div>
                          <div className="flex-1">
                            <p className="font-medium text-gray-900">
                              {user.first_name} {user.last_name}
                            </p>
                            <p className="text-sm text-gray-500">{user.email}</p>
                          </div>
                        </label>
                      ))
                    )}
                  </div>
                </div>

                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-2">
                    Role
                  </label>
                  <div className="space-y-2">
                    <label className="flex items-center gap-2">
                      <input
                        type="radio"
                        name="role"
                        value="member"
                        checked={selectedRole === "member"}
                        onChange={(e) => setSelectedRole(e.target.value as 'admin' | 'manager' | 'member')}
                        className="w-4 h-4 text-[#62ac4a] focus:ring-[#62ac4a]"
                      />
                      <span className="flex items-center gap-1.5">
                        <User className="w-4 h-4 text-gray-400" />
                        Member
                      </span>
                    </label>
                    <label className="flex items-center gap-2">
                      <input
                        type="radio"
                        name="role"
                        value="manager"
                        checked={selectedRole === "manager"}
                        onChange={(e) => setSelectedRole(e.target.value as 'admin' | 'manager' | 'member')}
                        className="w-4 h-4 text-[#62ac4a] focus:ring-[#62ac4a]"
                      />
                      <span className="flex items-center gap-1.5">
                        <Shield className="w-4 h-4 text-blue-500" />
                        Manager
                      </span>
                    </label>
                    <label className="flex items-center gap-2">
                      <input
                        type="radio"
                        name="role"
                        value="admin"
                        checked={selectedRole === "admin"}
                        onChange={(e) => setSelectedRole(e.target.value as 'admin' | 'manager' | 'member')}
                        className="w-4 h-4 text-[#62ac4a] focus:ring-[#62ac4a]"
                      />
                      <span className="flex items-center gap-1.5">
                        <Shield className="w-4 h-4 text-amber-500" />
                        Admin
                      </span>
                    </label>
                  </div>
                </div>
              </div>

              <div className="flex gap-3 p-6 border-t border-gray-200">
                <button
                  type="button"
                  onClick={() => setShowAddModal(false)}
                  className="flex-1 px-4 py-2 border border-gray-200 text-gray-700 rounded-lg hover:bg-gray-50 transition"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={!selectedUserId || addingMember}
                  className="flex-1 px-4 py-2 bg-[#62ac4a] text-white rounded-lg hover:bg-[#4e8a3a] transition disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {addingMember ? "Adding..." : "Add Member"}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </AdminLayout>
  );
}
