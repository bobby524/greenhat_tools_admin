"use client";

import { useEffect, useState } from "react";
import AdminLayout from "../components/AdminLayout";
import { 
  Users, 
  Plus, 
  Search, 
  FolderGit, 
  ChevronRight,
  Edit2,
  Trash2,
  Palette
} from "lucide-react";

interface Team {
  id: string;
  name: string;
  slug: string;
  color: string;
  org_id: string;
  created_at: string;
  member_count: number;
  project_count: number;
}

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || "";

export default function TeamsAdmin() {
  const [teams, setTeams] = useState<Team[]>([]);
  const [loading, setLoading] = useState(true);
  const [searchTerm, setSearchTerm] = useState("");
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Create team form state
  const [newTeam, setNewTeam] = useState({
    name: "",
    color: "#3b82f6",
  });

  useEffect(() => {
    loadTeams();
  }, []);

  async function loadTeams() {
    try {
      setLoading(true);
      const orgId = "cd861b76-f85c-4afc-b3e8-8f85945c3132"; // Default org
      const response = await fetch(`${API_BASE_URL}/api/admin/teams?org_id=${orgId}`);
      if (!response.ok) throw new Error("Failed to load teams");
      const data = await response.json();
      setTeams(data.teams || []);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  async function createTeam(e: React.FormEvent) {
    e.preventDefault();
    try {
      const orgId = "cd861b76-f85c-4afc-b3e8-8f85945c3132"; // Default org
      const response = await fetch(`${API_BASE_URL}/api/admin/teams`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: newTeam.name,
          color: newTeam.color,
          org_id: orgId,
        }),
      });
      if (!response.ok) throw new Error("Failed to create team");
      setShowCreateModal(false);
      setNewTeam({ name: "", color: "#3b82f6" });
      loadTeams();
    } catch (err: any) {
      setError(err.message);
    }
  }

  async function deleteTeam(teamId: string) {
    if (!confirm("Are you sure you want to delete this team? This action cannot be undone.")) return;
    try {
      const response = await fetch(`${API_BASE_URL}/api/admin/teams/${teamId}`, {
        method: "DELETE",
      });
      if (!response.ok) throw new Error("Failed to delete team");
      loadTeams();
    } catch (err: any) {
      setError(err.message);
    }
  }

  const filteredTeams = teams.filter(team =>
    team.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
    team.slug.toLowerCase().includes(searchTerm.toLowerCase())
  );

  const colorOptions = [
    { value: "#3b82f6", label: "Blue" },
    { value: "#10b981", label: "Green" },
    { value: "#f59e0b", label: "Amber" },
    { value: "#ef4444", label: "Red" },
    { value: "#8b5cf6", label: "Purple" },
    { value: "#ec4899", label: "Pink" },
    { value: "#06b6d4", label: "Cyan" },
    { value: "#6366f1", label: "Indigo" },
  ];

  return (
    <AdminLayout title="Team Management">
      {/* Header Actions */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-6">
        <div className="relative flex-1 max-w-md">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
          <input
            type="text"
            placeholder="Search teams..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            className="w-full pl-10 pr-4 py-2 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-[#62ac4a] focus:border-transparent"
          />
        </div>
        <button
          onClick={() => setShowCreateModal(true)}
          className="inline-flex items-center gap-2 px-4 py-2 bg-[#62ac4a] text-white rounded-lg hover:bg-[#4e8a3a] transition font-medium"
        >
          <Plus className="w-4 h-4" />
          Create Team
        </button>
      </div>

      {/* Error Alert */}
      {error && (
        <div className="mb-6 p-4 bg-red-50 border border-red-200 rounded-lg text-red-700">
          {error}
        </div>
      )}

      {/* Teams Grid */}
      {loading ? (
        <div className="flex items-center justify-center py-12">
          <div className="w-8 h-8 border-2 border-gray-300 border-t-[#62ac4a] rounded-full animate-spin" />
        </div>
      ) : filteredTeams.length === 0 ? (
        <div className="text-center py-12 bg-gray-50 rounded-xl border border-gray-200">
          <Users className="w-12 h-12 text-gray-300 mx-auto mb-4" />
          <h3 className="text-lg font-medium text-gray-900 mb-1">No teams found</h3>
          <p className="text-gray-500">
            {searchTerm ? "Try adjusting your search" : "Create your first team to get started"}
          </p>
        </div>
      ) : (
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
          {filteredTeams.map((team) => (
            <div
              key={team.id}
              className="group bg-white rounded-xl border border-gray-200 p-5 hover:border-[#62ac4a]/50 hover:shadow-lg hover:shadow-[#62ac4a]/5 transition-all duration-200"
            >
              <div className="flex items-start justify-between mb-4">
                <div className="flex items-center gap-3">
                  <div
                    className="w-10 h-10 rounded-lg flex items-center justify-center"
                    style={{ backgroundColor: `${team.color}15` }}
                  >
                    <Users className="w-5 h-5" style={{ color: team.color }} />
                  </div>
                  <div>
                    <h3 className="font-semibold text-gray-900">{team.name}</h3>
                    <p className="text-sm text-gray-500">@{team.slug}</p>
                  </div>
                </div>
                <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <button
                    onClick={() => deleteTeam(team.id)}
                    className="p-1.5 text-gray-400 hover:text-red-600 hover:bg-red-50 rounded-lg transition"
                    title="Delete team"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>

              <div className="flex items-center gap-4 text-sm text-gray-600 mb-4">
                <div className="flex items-center gap-1.5">
                  <Users className="w-4 h-4 text-gray-400" />
                  <span>{team.member_count} members</span>
                </div>
                <div className="flex items-center gap-1.5">
                  <FolderGit className="w-4 h-4 text-gray-400" />
                  <span>{team.project_count} projects</span>
                </div>
              </div>

              <a
                href={`/admin/teams/${team.id}`}
                className="flex items-center justify-between w-full px-4 py-2.5 bg-gray-50 hover:bg-[#62ac4a]/10 text-gray-700 hover:text-[#41734a] rounded-lg transition font-medium text-sm"
              >
                <span>Manage Team</span>
                <ChevronRight className="w-4 h-4" />
              </a>
            </div>
          ))}
        </div>
      )}

      {/* Create Team Modal */}
      {showCreateModal && (
        <div className="fixed inset-0 bg-black/50 z-50 flex items-center justify-center p-4">
          <div className="bg-white rounded-xl shadow-xl max-w-md w-full">
            <div className="flex items-center justify-between p-6 border-b border-gray-200">
              <h2 className="text-lg font-semibold text-gray-900">Create New Team</h2>
              <button
                onClick={() => setShowCreateModal(false)}
                className="text-gray-400 hover:text-gray-600"
              >
                ×
              </button>
            </div>
            <form onSubmit={createTeam} className="p-6 space-y-4">
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Team Name
                </label>
                <input
                  type="text"
                  required
                  value={newTeam.name}
                  onChange={(e) => setNewTeam({ ...newTeam, name: e.target.value })}
                  className="w-full px-3 py-2 border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-[#62ac4a] focus:border-transparent"
                  placeholder="e.g., Engineering"
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-2">
                  Team Color
                </label>
                <div className="flex flex-wrap gap-2">
                  {colorOptions.map((color) => (
                    <button
                      key={color.value}
                      type="button"
                      onClick={() => setNewTeam({ ...newTeam, color: color.value })}
                      className={`w-8 h-8 rounded-lg transition ${
                        newTeam.color === color.value
                          ? "ring-2 ring-offset-2 ring-gray-400"
                          : "hover:scale-110"
                      }`}
                      style={{ backgroundColor: color.value }}
                      title={color.label}
                    />
                  ))}
                </div>
              </div>
              <div className="flex gap-3 pt-4">
                <button
                  type="button"
                  onClick={() => setShowCreateModal(false)}
                  className="flex-1 px-4 py-2 border border-gray-200 text-gray-700 rounded-lg hover:bg-gray-50 transition"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  className="flex-1 px-4 py-2 bg-[#62ac4a] text-white rounded-lg hover:bg-[#4e8a3a] transition"
                >
                  Create Team
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </AdminLayout>
  );
}
