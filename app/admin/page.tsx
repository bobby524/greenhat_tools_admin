"use client";

import AdminLayout from "./components/AdminLayout";
import { Shield, Users, Lock, CheckCircle, Database, Server, Globe, Users2, FolderGit } from "lucide-react";

// Green color palette matching tools.greenhatsec.com
const COLORS = {
  primary: "#62ac4a",
  primaryHover: "#4e8a3a",
  primaryDeep: "#41734a",
  primaryLight: "#8bcd7b",
};

export default function AdminDashboard() {
  return (
    <AdminLayout title="Dashboard">
      {/* Module Cards Grid */}
      <div className="grid gap-6 md:grid-cols-2 xl:grid-cols-3">
        {/* Teams Management Card */}
        <DashboardCard
          title="Teams"
          description="Create and manage teams, assign members, and organize projects"
          icon={FolderGit}
          href="/admin/teams"
          status="active"
          color={COLORS.primary}
        />

        {/* User Management Card */}
        <DashboardCard
          title="Users"
          description="View all users, manage team memberships and organization access"
          icon={Users2}
          href="/admin/users"
          status="active"
          color={COLORS.primary}
        />

        {/* MCP Firewall Card */}
        <DashboardCard
          title="MCP Firewall"
          description="Monitor and control agent data access with real-time security monitoring"
          icon={Shield}
          href="/admin/mcp-firewall"
          status="active"
          color={COLORS.primary}
        />

        {/* Greenspot Admin Card */}
        <DashboardCard
          title="Greenspot"
          description="Manage Greenspot settings, pipelines, fields, and system configuration"
          icon={Users}
          href="/admin/greenspot"
          status="active"
          color={COLORS.primary}
        />

        {/* Access Controls Card */}
        <DashboardCard
          title="Access Controls"
          description="Manage user roles, permissions, and authentication settings"
          icon={Lock}
          href="/admin/access-controls"
          status="active"
          color={COLORS.primary}
        />
      </div>

      {/* System Status Section */}
      <div className="mt-8">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">System Status</h2>
        <div className="bg-white rounded-xl border border-gray-200 p-6">
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <StatusItem 
              label="Authentication" 
              status="online" 
              icon={CheckCircle}
            />
            <StatusItem 
              label="Database" 
              status="online" 
              icon={Database}
            />
            <StatusItem 
              label="MCP Server" 
              status="online" 
              icon={Server}
            />
            <StatusItem 
              label="API Gateway" 
              status="online" 
              icon={Globe}
            />
          </div>
        </div>
      </div>

      {/* Quick Actions */}
      <div className="mt-8">
        <h2 className="text-lg font-semibold text-gray-900 mb-4">Quick Actions</h2>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <QuickAction 
            label="Manage Teams"
            href="/admin/teams"
            description="Create and edit teams"
          />
          <QuickAction 
            label="View All Users"
            href="/admin/users"
            description="User directory"
          />
          <QuickAction 
            label="View Firewall Logs"
            href="/admin/mcp-firewall"
            description="Check recent activity"
          />
          <QuickAction 
            label="Greenspot Settings"
            href="/admin/greenspot"
            description="Configure Greenspot options"
          />
        </div>
      </div>
    </AdminLayout>
  );
}

function DashboardCard({
  title,
  description,
  icon: Icon,
  href,
  status,
  color,
}: {
  title: string;
  description: string;
  icon: React.ComponentType<{ className?: string }>;
  href: string;
  status: "active" | "coming-soon";
  color: string;
}) {
  const isDisabled = status === "coming-soon";
  
  return (
    <a
      href={href}
      onClick={(e) => isDisabled && e.preventDefault()}
      className={`group block bg-white rounded-xl border border-gray-200 p-6 transition-all duration-200 ${
        isDisabled 
          ? "opacity-60 cursor-not-allowed" 
          : "hover:border-[#62ac4a]/50 hover:shadow-lg hover:shadow-[#62ac4a]/5"
      }`}
    >
      <div className="flex items-start justify-between mb-4">
        <div 
          className="w-12 h-12 rounded-xl flex items-center justify-center transition-transform group-hover:scale-105"
          style={{ backgroundColor: `${color}15` }}
        >
          <div style={{ color }}><Icon className="w-6 h-6" /></div>
        </div>
        {status === "coming-soon" && (
          <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-amber-100 text-amber-800">
            Coming Soon
          </span>
        )}
        {status === "active" && (
          <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-green-100 text-green-800">
            Active
          </span>
        )}
      </div>
      <h3 className="text-lg font-semibold text-gray-900 mb-2">{title}</h3>
      <p className="text-sm text-gray-600 leading-relaxed">{description}</p>
      
      {!isDisabled && (
        <div className="mt-4 flex items-center text-sm font-medium" style={{ color }}>
          Open module
          <svg className="w-4 h-4 ml-1 transition-transform group-hover:translate-x-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        </div>
      )}
    </a>
  );
}

function StatusItem({ 
  label, 
  status, 
  icon: Icon 
}: { 
  label: string; 
  status: "online" | "offline";
  icon: React.ComponentType<{ className?: string }>;
}) {
  const isOnline = status === "online";
  
  return (
    <div className="flex items-center gap-3 p-3 rounded-lg bg-gray-50">
      <div className={`w-10 h-10 rounded-lg flex items-center justify-center ${
        isOnline ? "bg-green-100" : "bg-red-100"
      }`}>
        <Icon className={`w-5 h-5 ${isOnline ? "text-green-600" : "text-red-600"}`} />
      </div>
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-gray-900">{label}</p>
        <div className="flex items-center gap-1.5 mt-0.5">
          <span className={`w-2 h-2 rounded-full ${isOnline ? "bg-green-500" : "bg-red-500"}`} />
          <span className={`text-xs font-medium ${isOnline ? "text-green-600" : "text-red-600"}`}>
            {isOnline ? "Online" : "Offline"}
          </span>
        </div>
      </div>
    </div>
  );
}

function QuickAction({
  label,
  href,
  description,
}: {
  label: string;
  href: string;
  description: string;
}) {
  return (
    <a
      href={href}
      className="group block p-4 rounded-xl border border-gray-200 bg-white hover:border-[#62ac4a]/50 hover:shadow-md transition-all duration-200"
    >
      <h4 className="text-sm font-semibold text-gray-900 group-hover:text-[#41734a] transition-colors">
        {label}
      </h4>
      <p className="text-xs text-gray-500 mt-1">{description}</p>
    </a>
  );
}
