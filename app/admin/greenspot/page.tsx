"use client";

import { useEffect, useState } from "react";
import AdminLayout from "../components/AdminLayout";
import { 
  Users, 
  Building2, 
  Briefcase, 
  CheckCircle,
  Settings,
  ExternalLink,
  Loader2
} from "lucide-react";

// Green color palette
const COLORS = {
  primary: "#62ac4a",
  primaryHover: "#4e8a3a",
  primaryDeep: "#41734a",
  mint: "#e8f5e9",
  surface: "#f1f8f2",
  border: "#c8e6c9",
};

interface Stats {
  contacts: number;
  companies: number;
  deals: number;
  tasks: number;
}

export default function GreenSpotAdminPage() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(true);

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
          <a
            href="https://tools.greenhatsec.com/greenspot"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-2 px-4 py-2 bg-[#62ac4a] text-white rounded-lg hover:bg-[#4e8a3a] transition font-medium"
          >
            Open GreenSpot
            <ExternalLink className="w-4 h-4" />
          </a>
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

        {/* Settings Links */}
        <div className="bg-white rounded-xl border border-gray-200 p-6">
          <h2 className="text-lg font-semibold text-gray-900 mb-4">Settings</h2>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            <SettingsCard
              title="Deal Pipelines"
              description="Configure sales pipelines and stages"
              href="https://tools.greenhatsec.com/greenspot/settings"
              icon={Briefcase}
            />
            <SettingsCard
              title="Contact Fields"
              description="Customize contact record fields"
              href="https://tools.greenhatsec.com/greenspot/settings"
              icon={Users}
            />
            <SettingsCard
              title="Company Fields"
              description="Customize company record fields"
              href="https://tools.greenhatsec.com/greenspot/settings"
              icon={Building2}
            />
          </div>
        </div>

        {/* Info Banner */}
        <div className="bg-amber-50 border border-amber-200 rounded-xl p-4">
          <div className="flex items-start gap-3">
            <Settings className="w-5 h-5 text-amber-600 mt-0.5" />
            <div>
              <h3 className="font-semibold text-amber-800">Settings Migration in Progress</h3>
              <p className="text-sm text-amber-700 mt-1">
                Full settings workspace is being migrated from tools.greenhatsec.com to admin.greenhatsec.com. 
                Click the settings cards above to access the current settings page.
              </p>
            </div>
          </div>
        </div>
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

function SettingsCard({
  title,
  description,
  href,
  icon: Icon,
}: {
  title: string;
  description: string;
  href: string;
  icon: React.ComponentType<{ className?: string }>;
}) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className="group block p-4 rounded-xl border border-gray-200 hover:border-[#62ac4a] hover:shadow-md transition-all"
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
        <ExternalLink className="w-4 h-4 text-gray-400 group-hover:text-[#62ac4a]" />
      </div>
    </a>
  );
}
