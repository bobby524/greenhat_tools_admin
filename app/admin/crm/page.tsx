"use client";

import AdminLayout from "../components/AdminLayout";
import { 
  Users, 
  BarChart3, 
  RefreshCw, 
  Settings, 
  Mail, 
  Database,
  ArrowRight,
  Construction,
  CheckCircle2,
  Clock,
  Circle
} from "lucide-react";

export default function CRMAdminModule() {
  return (
    <AdminLayout title="CRM Admin">
      {/* Coming Soon Banner */}
      <div className="relative overflow-hidden rounded-2xl bg-gradient-to-br from-[#41734a] to-[#62ac4a] p-8 mb-8">
        <div className="absolute top-0 right-0 -mt-8 -mr-8 w-32 h-32 bg-white/10 rounded-full blur-2xl" />
        <div className="absolute bottom-0 left-0 -mb-8 -ml-8 w-24 h-24 bg-white/10 rounded-full blur-xl" />
        <div className="relative flex items-start gap-4">
          <div className="w-14 h-14 bg-white/20 rounded-xl flex items-center justify-center flex-shrink-0">
            <Construction className="w-7 h-7 text-white" />
          </div>
          <div>
            <h2 className="text-2xl font-bold text-white mb-2">Coming Soon</h2>
            <p className="text-white/90 text-lg">
              The CRM Admin module is currently being migrated from tools.greenhatsec.com
            </p>
          </div>
        </div>
      </div>

      {/* Feature Preview Grid */}
      <div className="mb-8">
        <h3 className="text-lg font-semibold text-gray-900 mb-4">Planned Features</h3>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          <FeatureCard
            icon={Users}
            title="User Management"
            description="Manage CRM users, roles, and permissions with granular access controls"
          />
          <FeatureCard
            icon={BarChart3}
            title="Analytics Dashboard"
            description="View CRM metrics, performance analytics, and custom reports"
          />
          <FeatureCard
            icon={RefreshCw}
            title="Data Sync"
            description="Configure data synchronization settings and integration workflows"
          />
          <FeatureCard
            icon={Settings}
            title="System Settings"
            description="Manage CRM configuration, preferences, and global options"
          />
          <FeatureCard
            icon={Mail}
            title="Email Templates"
            description="Customize email templates and notification preferences"
          />
          <FeatureCard
            icon={Database}
            title="Custom Fields"
            description="Manage custom fields, data structures, and field mappings"
          />
        </div>
      </div>

      {/* Current Location Card */}
      <div className="bg-green-50 border border-green-200 rounded-xl p-6 mb-8">
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
          <div className="flex items-start gap-4">
            <div className="w-12 h-12 bg-green-100 rounded-xl flex items-center justify-center flex-shrink-0">
              <ArrowRight className="w-6 h-6 text-[#41734a]" />
            </div>
            <div>
              <h4 className="text-lg font-semibold text-gray-900">Looking for CRM Admin?</h4>
              <p className="text-gray-600 mt-1">
                The admin settings are currently located at tools.greenhatsec.com
              </p>
            </div>
          </div>
          <a
            href="https://tools.greenhatsec.com/admin"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center justify-center gap-2 px-5 py-2.5 bg-[#62ac4a] text-white rounded-lg hover:bg-[#4e8a3a] transition font-medium whitespace-nowrap"
          >
            Go to CRM Admin
            <ArrowRight className="w-4 h-4" />
          </a>
        </div>
      </div>

      {/* Migration Progress */}
      <div className="bg-white rounded-xl border border-gray-200 overflow-hidden">
        <div className="p-6 border-b border-gray-200">
          <h4 className="text-lg font-semibold text-gray-900">Migration Progress</h4>
        </div>
        <div className="p-6">
          {/* Overall Progress Bar */}
          <div className="mb-6">
            <div className="flex items-center justify-between mb-2">
              <span className="text-sm font-medium text-gray-700">Overall Progress</span>
              <span className="text-sm font-bold text-[#62ac4a]">25%</span>
            </div>
            <div className="h-2 bg-gray-200 rounded-full overflow-hidden">
              <div 
                className="h-full bg-gradient-to-r from-[#62ac4a] to-[#8bcd7b] rounded-full transition-all duration-500"
                style={{ width: "25%" }}
              />
            </div>
          </div>

          {/* Progress Items */}
          <div className="space-y-3">
            <ProgressItem label="UI Components" status="completed" />
            <ProgressItem label="API Integration" status="in-progress" />
            <ProgressItem label="Data Migration" status="pending" />
            <ProgressItem label="Testing & QA" status="pending" />
          </div>
        </div>
      </div>
    </AdminLayout>
  );
}

function FeatureCard({
  icon: Icon,
  title,
  description,
}: {
  icon: React.ComponentType<{ className?: string }>;
  title: string;
  description: string;
}) {
  return (
    <div className="group p-5 bg-white rounded-xl border border-gray-200 hover:border-[#62ac4a]/50 hover:shadow-lg hover:shadow-[#62ac4a]/5 transition-all duration-200">
      <div className="w-12 h-12 bg-[#62ac4a]/10 rounded-xl flex items-center justify-center mb-4 group-hover:scale-105 transition-transform">
        <Icon className="w-6 h-6 text-[#62ac4a]" />
      </div>
      <h4 className="text-base font-semibold text-gray-900 mb-2">{title}</h4>
      <p className="text-sm text-gray-600 leading-relaxed">{description}</p>
    </div>
  );
}

function ProgressItem({ 
  label, 
  status 
}: { 
  label: string; 
  status: "completed" | "in-progress" | "pending";
}) {
  const statusConfig = {
    completed: {
      icon: CheckCircle2,
      color: "text-green-600",
      bgColor: "bg-green-100",
      borderColor: "border-green-200",
      label: "Completed",
      labelColor: "text-green-700",
    },
    "in-progress": {
      icon: Clock,
      color: "text-blue-600",
      bgColor: "bg-blue-100",
      borderColor: "border-blue-200",
      label: "In Progress",
      labelColor: "text-blue-700",
    },
    pending: {
      icon: Circle,
      color: "text-gray-400",
      bgColor: "bg-gray-100",
      borderColor: "border-gray-200",
      label: "Pending",
      labelColor: "text-gray-600",
    },
  };

  const config = statusConfig[status];
  const Icon = config.icon;

  return (
    <div className="flex items-center gap-4 p-3 rounded-lg hover:bg-gray-50 transition-colors">
      <div className={`w-10 h-10 rounded-lg flex items-center justify-center flex-shrink-0 ${config.bgColor}`}>
        <Icon className={`w-5 h-5 ${config.color}`} />
      </div>
      <span className="flex-1 font-medium text-gray-900">{label}</span>
      <span className={`px-2.5 py-1 rounded-full text-xs font-semibold border ${config.borderColor} ${config.labelColor}`}>
        {config.label}
      </span>
    </div>
  );
}
