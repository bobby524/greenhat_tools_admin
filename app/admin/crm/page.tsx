"use client";

import { useState, useEffect } from "react";
import AdminLayout from "../components/AdminLayout";
import { 
  Users, 
  Building2, 
  Briefcase, 
  CheckCircle,
  Plus,
  Search,
  Loader2,
  Mail,
  Phone
} from "lucide-react";

interface Contact {
  id: string;
  name: string;
  email: string;
  phone: string | null;
  company_name: string | null;
  lifecycle_stage: string;
  created_at: string;
}

interface Company {
  id: string;
  name: string;
  domain: string | null;
  industry: string | null;
  size: string | null;
  created_at: string;
}

export default function CRMPage() {
  const [activeTab, setActiveTab] = useState<"contacts" | "companies">("contacts");
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [companies, setCompanies] = useState<Company[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");

  useEffect(() => {
    if (activeTab === "contacts") {
      fetchContacts();
    } else {
      fetchCompanies();
    }
  }, [activeTab, search]);

  async function fetchContacts() {
    setLoading(true);
    try {
      const response = await fetch(`/api/crm/contacts?search=${encodeURIComponent(search)}`, {
        credentials: "include",
      });
      if (!response.ok) throw new Error("Failed to fetch contacts");
      const data = await response.json();
      setContacts(data.contacts || []);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Error loading contacts");
    } finally {
      setLoading(false);
    }
  }

  async function fetchCompanies() {
    setLoading(true);
    try {
      const response = await fetch(`/api/crm/companies?search=${encodeURIComponent(search)}`, {
        credentials: "include",
      });
      if (!response.ok) throw new Error("Failed to fetch companies");
      const data = await response.json();
      setCompanies(data.companies || []);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Error loading companies");
    } finally {
      setLoading(false);
    }
  }

  return (
    <AdminLayout title="CRM">
      <div className="space-y-6">
        {/* Header */}
        <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
          <div>
            <h1 className="text-2xl font-bold text-gray-900">CRM</h1>
            <p className="text-gray-600 mt-1">Manage your contacts and companies</p>
          </div>
          <button className="inline-flex items-center gap-2 px-4 py-2 bg-[#62ac4a] text-white rounded-lg hover:bg-[#4e8a3a] transition font-medium">
            <Plus className="w-4 h-4" />
            Add {activeTab === "contacts" ? "Contact" : "Company"}
          </button>
        </div>

        {/* Stats */}
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <StatCard title="Total Contacts" value="-" icon={Users} loading={loading} />
          <StatCard title="Companies" value="-" icon={Building2} loading={loading} />
          <StatCard title="Deals" value="-" icon={Briefcase} loading={loading} />
          <StatCard title="Tasks" value="-" icon={CheckCircle} loading={loading} />
        </div>

        {/* Tabs & Search */}
        <div className="bg-white rounded-xl border border-gray-200 overflow-hidden">
          <div className="p-4 border-b border-gray-200 flex flex-col sm:flex-row sm:items-center gap-4">
            <div className="flex gap-2">
              <TabButton 
                active={activeTab === "contacts"} 
                onClick={() => setActiveTab("contacts")}
                icon={Users}
                label="Contacts"
              />
              <TabButton 
                active={activeTab === "companies"} 
                onClick={() => setActiveTab("companies")}
                icon={Building2}
                label="Companies"
              />
            </div>
            <div className="relative flex-1 max-w-md">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
              <input
                type="text"
                placeholder={`Search ${activeTab}...`}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="w-full pl-10 pr-4 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-[#62ac4a] focus:border-transparent"
              />
            </div>
          </div>

          {/* Data Table */}
          <div className="overflow-x-auto">
            {loading ? (
              <div className="p-12 flex items-center justify-center">
                <Loader2 className="w-8 h-8 animate-spin text-[#62ac4a]" />
              </div>
            ) : error ? (
              <div className="p-12 text-center text-red-600">
                <p>{error}</p>
                <button 
                  onClick={() => activeTab === "contacts" ? fetchContacts() : fetchCompanies()}
                  className="mt-2 text-sm text-[#62ac4a] hover:underline"
                >
                  Retry
                </button>
              </div>
            ) : activeTab === "contacts" ? (
              <ContactsTable contacts={contacts} />
            ) : (
              <CompaniesTable companies={companies} />
            )}
          </div>
        </div>
      </div>
    </AdminLayout>
  );
}

function StatCard({ title, value, icon: Icon, loading }: { 
  title: string; 
  value: string; 
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

function TabButton({ active, onClick, icon: Icon, label }: {
  active: boolean;
  onClick: () => void;
  icon: React.ComponentType<{ className?: string }>;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      className={`inline-flex items-center gap-2 px-4 py-2 rounded-lg font-medium transition ${
        active 
          ? "bg-[#62ac4a] text-white" 
          : "bg-gray-100 text-gray-700 hover:bg-gray-200"
      }`}
    >
      <Icon className="w-4 h-4" />
      {label}
    </button>
  );
}

function ContactsTable({ contacts }: { contacts: Contact[] }) {
  if (contacts.length === 0) {
    return (
      <div className="p-12 text-center text-gray-500">
        <Users className="w-12 h-12 mx-auto mb-4 text-gray-300" />
        <p className="text-lg font-medium">No contacts found</p>
        <p className="text-sm mt-1">Add your first contact to get started</p>
      </div>
    );
  }

  return (
    <table className="w-full">
      <thead className="bg-gray-50 border-b border-gray-200">
        <tr>
          <th className="px-6 py-3 text-left text-xs font-semibold text-gray-600 uppercase">Name</th>
          <th className="px-6 py-3 text-left text-xs font-semibold text-gray-600 uppercase">Email</th>
          <th className="px-6 py-3 text-left text-xs font-semibold text-gray-600 uppercase">Phone</th>
          <th className="px-6 py-3 text-left text-xs font-semibold text-gray-600 uppercase">Company</th>
          <th className="px-6 py-3 text-left text-xs font-semibold text-gray-600 uppercase">Stage</th>
        </tr>
      </thead>
      <tbody className="divide-y divide-gray-200">
        {contacts.map((contact) => (
          <tr key={contact.id} className="hover:bg-gray-50">
            <td className="px-6 py-4 font-medium text-gray-900">{contact.name}</td>
            <td className="px-6 py-4">
              <a href={`mailto:${contact.email}`} className="text-[#62ac4a] hover:underline flex items-center gap-1">
                <Mail className="w-4 h-4" />
                {contact.email}
              </a>
            </td>
            <td className="px-6 py-4 text-gray-600">
              {contact.phone ? (
                <a href={`tel:${contact.phone}`} className="flex items-center gap-1 hover:text-[#62ac4a]">
                  <Phone className="w-4 h-4" />
                  {contact.phone}
                </a>
              ) : (
                <span className="text-gray-400">-</span>
              )}
            </td>
            <td className="px-6 py-4 text-gray-600">{contact.company_name || "-"}</td>
            <td className="px-6 py-4">
              <span className="inline-flex px-2 py-1 text-xs font-medium rounded-full bg-[#62ac4a]/10 text-[#41734a]">
                {contact.lifecycle_stage}
              </span>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function CompaniesTable({ companies }: { companies: Company[] }) {
  if (companies.length === 0) {
    return (
      <div className="p-12 text-center text-gray-500">
        <Building2 className="w-12 h-12 mx-auto mb-4 text-gray-300" />
        <p className="text-lg font-medium">No companies found</p>
        <p className="text-sm mt-1">Add your first company to get started</p>
      </div>
    );
  }

  return (
    <table className="w-full">
      <thead className="bg-gray-50 border-b border-gray-200">
        <tr>
          <th className="px-6 py-3 text-left text-xs font-semibold text-gray-600 uppercase">Name</th>
          <th className="px-6 py-3 text-left text-xs font-semibold text-gray-600 uppercase">Domain</th>
          <th className="px-6 py-3 text-left text-xs font-semibold text-gray-600 uppercase">Industry</th>
          <th className="px-6 py-3 text-left text-xs font-semibold text-gray-600 uppercase">Size</th>
        </tr>
      </thead>
      <tbody className="divide-y divide-gray-200">
        {companies.map((company) => (
          <tr key={company.id} className="hover:bg-gray-50">
            <td className="px-6 py-4 font-medium text-gray-900">{company.name}</td>
            <td className="px-6 py-4">
              {company.domain ? (
                <a 
                  href={`https://${company.domain}`} 
                  target="_blank" 
                  rel="noopener noreferrer"
                  className="text-[#62ac4a] hover:underline"
                >
                  {company.domain}
                </a>
              ) : (
                <span className="text-gray-400">-</span>
              )}
            </td>
            <td className="px-6 py-4 text-gray-600">{company.industry || "-"}</td>
            <td className="px-6 py-4 text-gray-600">{company.size || "-"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
