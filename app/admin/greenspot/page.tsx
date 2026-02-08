"use client";

import { useEffect, useState } from "react";
import AdminLayout from "../components/AdminLayout";
import SettingsWorkspace from "./SettingsWorkspace";
import { 
  Users, 
  Building2, 
  Briefcase,
  CheckCircle,
  ExternalLink,
  Loader2,
  Settings,
  ArrowRight
} from "lucide-react";

interface Stats {
  contacts: number;
  companies: number;
  deals: number;
  tasks: number;
}

export default function GreenSpotAdminPage() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(true);
  const [showSettings, setShowSettings] = useState(false);

  useEffect(() => {
    fetchStats();
  }, []);

  async function fetchStats() {
    try {
      const response = await fetch("https://tools.greenhatsec.com/api/greenspot/dashboard");
      if (!response.ok) throw new Error("Failed to fetch stats");
      const data = await response.json();
      setStats(data.stats);
    } catch (err) {
      console.error("Error fetching stats:", err);
    } finally {
      setLoading(false);
    }
  }

  return (
    <AdminLayout title="GreenSpot Admin">
      <div className="space-y-6">
        {/* Header */}
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
          <div>
            <h1 className="text-2xl font-bold text-gray-900">GreenSpot Admin</h1>
            <p className="text-gray-600 mt-1">Manage CRM settings, pipelines, and field customization</p>
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={() => setShowSettings(!showSettings)}
              className={`inline-flex items-center gap-2 px-4 py-2 rounded-lg font-medium transition ${
                showSettings 
                  ? "bg-gray-100 text-gray-700 hover:bg-gray-200" 
                  : "bg-[#62ac4a] text-white hover:bg-[#4e8a3a]"
              }`}
            >
              <Settings className="w-4 h-4" />
              {showSettings ? "Hide Settings" : "Edit Settings"}
            </button>
            <a
              href="https://tools.greenhatsec.com/greenspot"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-2 px-4 py-2 bg-white border border-gray-200 text-gray-700 rounded-lg hover:border-[#62ac4a] hover:text-[#62ac4a] transition font-medium"
            >
              Open GreenSpot
              <ExternalLink className="w-4 h-4" />
            </a>
          </div>
        </div>

        {/* Stats Grid */}
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <StatCard 
            title="Total Contacts" 
            value={stats?.contacts ?? 0} 
            icon={Users} 
            loading={loading} 
          />
          <StatCard 
            title="Companies" 
            value={stats?.companies ?? 0} 
            icon={Building2} 
            loading={loading} 
          />
          <StatCard 
            title="Deals" 
            value={stats?.deals ?? 0} 
            icon={Briefcase} 
            loading={loading} 
          />
          <StatCard 
            title="Tasks" 
            value={stats?.tasks ?? 0} 
            icon={CheckCircle} 
            loading={loading} 
          />
        </div>

        {/* Success Banner */}
        <div className="bg-green-50 border border-green-200 rounded-xl p-4">
          <div className="flex items-start gap-3">
            <div className="w-5 h-5 rounded-full bg-[#62ac4a] flex items-center justify-center flex-shrink-0 mt-0.5">
              <CheckCircle className="w-3 h-3 text-white" />
            </div>
            <div>
              <h3 className="font-semibold text-green-800">Database Connected</h3>
              <p className="text-sm text-green-700 mt-1">
                All settings changes are now saved directly to the Supabase database. 
                Changes made here will immediately reflect on tools.greenhatsec.com.
              </p>
            </div>
          </div>
        </div>

        {/* Settings Workspace */}
        {showSettings && (
          <div className="border-t border-gray-200 pt-6">
            <SettingsWorkspace />
          </div>
        )}

        {/* Quick Actions */}
        {!showSettings && (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            <QuickActionCard
              title="Deal Pipelines"
              description="Configure sales pipelines and stages"
              onClick={() => setShowSettings(true)}
              icon={Briefcase}
            />
            <QuickActionCard
              title="Contact Fields"
              description="Customize contact record fields"
              onClick={() => setShowSettings(true)}
              icon={Users}
            />
            <QuickActionCard
              title="Company Fields"
              description="Customize company record fields"
              onClick={() => setShowSettings(true)}
              icon={Building2}
            />
          </div>
        )}
      </div>
    </AdminLayout>
  );
}

function StatCard({ 
  title, 
  value, 
  icon: Icon, 
  loading 
}: { 
  title: string; 
  value: number; 
  icon: React.ComponentType<{ className?: string }>;
  loading: boolean;
}) {
  return (
    <div className="bg-white rounded-xl border border-gray-200 p-6">
      <div className="flex items-center justify-between">
        <div>
          <p className="text-sm font-medium text-gray-600">{title}</p>
          <p className="text-2xl font-bold text-gray-900 mt-1">
            {loading ? <Loader2 className="w-6 h-6 animate-spin text-[#62ac4a]" /> : value}
          </p>
        </div>
        <div className="w-12 h-12 bg-[#62ac4a]/10 rounded-xl flex items-center justify-center">
          <Icon className="w-6 h-6 text-[#62ac4a]" />
        </div>
      </div>
    </div>
  );
}

function QuickActionCard({
  title,
  description,
  onClick,
  icon: Icon,
}: {
  title: string;
  description: string;
  onClick: () => void;
  icon: React.ComponentType<{ className?: string }>;
}) {
  return (
    <button
      onClick={onClick}
      className="group block w-full text-left p-4 rounded-xl border border-gray-200 hover:border-[#62ac4a] hover:shadow-md transition-all bg-white"
    >
      <div className="flex items-start gap-3">
        <div className="w-10 h-10 bg-[#62ac4a]/10 rounded-lg flex items-center justify-center">
          <Icon className="w-5 h-5 text-[#62ac4a]" />
        </div>
        <div className="flex-1">
          <h3 className="font-semibold text-gray-900 group-hover:text-[#62ac4a] transition-colors">
            {title}
          </h3>
          <p className="text-sm text-gray-600 mt-1">{description}</p>
        </div>
        <ArrowRight className="w-4 h-4 text-gray-400 group-hover:text-[#62ac4a] transition-colors" />
      </div>
    </button>
  );
}
