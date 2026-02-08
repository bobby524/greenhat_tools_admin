export type Json =
  | string
  | number
  | boolean
  | null
  | { [key: string]: Json | undefined }
  | Json[];

export type Database = {
  public: {
    Tables: {
      crm_activities: {
        Row: {
          activity_type: string;
          archived_at: string | null;
          body: string | null;
          created_at: string;
          created_by: string | null;
          id: string;
          occurred_at: string;
          related_company_id: string | null;
          related_contact_id: string | null;
          related_deal_id: string | null;
          updated_at: string;
          updated_by: string | null;
        };
        Insert: {
          activity_type: string;
          archived_at?: string | null;
          body?: string | null;
          created_at?: string;
          created_by?: string | null;
          id?: string;
          occurred_at?: string;
          related_company_id?: string | null;
          related_contact_id?: string | null;
          related_deal_id?: string | null;
          updated_at?: string;
          updated_by?: string | null;
        };
        Update: {
          activity_type?: string;
          archived_at?: string | null;
          body?: string | null;
          created_at?: string;
          created_by?: string | null;
          id?: string;
          occurred_at?: string;
          related_company_id?: string | null;
          related_contact_id?: string | null;
          related_deal_id?: string | null;
          updated_at?: string;
          updated_by?: string | null;
        };
        Relationships: [];
      };
      crm_audit_events: {
        Row: {
          actor_id: string | null;
          created_at: string;
          entity_id: string;
          entity_type: string;
          event_type: string;
          id: string;
          payload: Json;
        };
        Insert: {
          actor_id?: string | null;
          created_at?: string;
          entity_id: string;
          entity_type: string;
          event_type: string;
          id?: string;
          payload?: Json;
        };
        Update: {
          actor_id?: string | null;
          created_at?: string;
          entity_id?: string;
          entity_type?: string;
          event_type?: string;
          id?: string;
          payload?: Json;
        };
        Relationships: [];
      };
      crm_companies: {
        Row: {
          archived_at: string | null;
          created_at: string;
          created_by: string | null;
          custom_properties: Json;
          domain: string | null;
          id: string;
          name: string;
          phone: string | null;
          updated_at: string;
          updated_by: string | null;
        };
        Insert: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          custom_properties?: Json;
          domain?: string | null;
          id?: string;
          name: string;
          phone?: string | null;
          updated_at?: string;
          updated_by?: string | null;
        };
        Update: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          custom_properties?: Json;
          domain?: string | null;
          id?: string;
          name?: string;
          phone?: string | null;
          updated_at?: string;
          updated_by?: string | null;
        };
        Relationships: [];
      };
      crm_contacts: {
        Row: {
          archived_at: string | null;
          company_id: string | null;
          created_at: string;
          created_by: string | null;
          custom_properties: Json;
          email: string | null;
          id: string;
          name: string;
          phone: string | null;
          title: string | null;
          updated_at: string;
          updated_by: string | null;
        };
        Insert: {
          archived_at?: string | null;
          company_id?: string | null;
          created_at?: string;
          created_by?: string | null;
          custom_properties?: Json;
          email?: string | null;
          id?: string;
          name: string;
          phone?: string | null;
          title?: string | null;
          updated_at?: string;
          updated_by?: string | null;
        };
        Update: {
          archived_at?: string | null;
          company_id?: string | null;
          created_at?: string;
          created_by?: string | null;
          custom_properties?: Json;
          email?: string | null;
          id?: string;
          name?: string;
          phone?: string | null;
          title?: string | null;
          updated_at?: string;
          updated_by?: string | null;
        };
        Relationships: [];
      };
      crm_deal_pipeline_stages: {
        Row: {
          archived_at: string | null;
          created_at: string;
          created_by: string | null;
          id: string;
          is_active: boolean;
          is_closed_lost: boolean;
          is_closed_won: boolean;
          label: string;
          order: number;
          pipeline_id: string;
          stage_key: string;
          updated_at: string;
          updated_by: string | null;
        };
        Insert: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          id?: string;
          is_active?: boolean;
          is_closed_lost?: boolean;
          is_closed_won?: boolean;
          label: string;
          order: number;
          pipeline_id: string;
          stage_key: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Update: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          id?: string;
          is_active?: boolean;
          is_closed_lost?: boolean;
          is_closed_won?: boolean;
          label?: string;
          order?: number;
          pipeline_id?: string;
          stage_key?: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Relationships: [];
      };
      crm_deal_pipelines: {
        Row: {
          created_at: string;
          created_by: string | null;
          description: string | null;
          id: string;
          name: string;
          updated_at: string;
          updated_by: string | null;
        };
        Insert: {
          created_at?: string;
          created_by?: string | null;
          description?: string | null;
          id?: string;
          name: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Update: {
          created_at?: string;
          created_by?: string | null;
          description?: string | null;
          id?: string;
          name?: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Relationships: [];
      };
      crm_deal_stages: {
        Row: {
          created_at: string;
          id: string;
          is_closed_lost: boolean;
          is_closed_won: boolean;
          key: string;
          label: string;
          order: number;
          updated_at: string;
        };
        Insert: {
          created_at?: string;
          id?: string;
          is_closed_lost?: boolean;
          is_closed_won?: boolean;
          key: string;
          label: string;
          order?: number;
          updated_at?: string;
        };
        Update: {
          created_at?: string;
          id?: string;
          is_closed_lost?: boolean;
          is_closed_won?: boolean;
          key?: string;
          label?: string;
          order?: number;
          updated_at?: string;
        };
        Relationships: [];
      };
      crm_deals: {
        Row: {
          archived_at: string | null;
          company_id: string | null;
          contact_id: string | null;
          created_at: string;
          created_by: string | null;
          custom_properties: Json;
          id: string;
          pipeline_id: string | null;
          stage_key: string | null;
          title: string;
          updated_at: string;
          updated_by: string | null;
          value: number | null;
        };
        Insert: {
          archived_at?: string | null;
          company_id?: string | null;
          contact_id?: string | null;
          created_at?: string;
          created_by?: string | null;
          custom_properties?: Json;
          id?: string;
          pipeline_id?: string | null;
          stage_key?: string | null;
          title: string;
          updated_at?: string;
          updated_by?: string | null;
          value?: number | null;
        };
        Update: {
          archived_at?: string | null;
          company_id?: string | null;
          contact_id?: string | null;
          created_at?: string;
          created_by?: string | null;
          custom_properties?: Json;
          id?: string;
          pipeline_id?: string | null;
          stage_key?: string | null;
          title?: string;
          updated_at?: string;
          updated_by?: string | null;
          value?: number | null;
        };
        Relationships: [];
      };
      crm_field_definitions: {
        Row: {
          archived_at: string | null;
          created_at: string;
          created_by: string | null;
          default_value: Json | null;
          field_key: string;
          field_type: string;
          id: string;
          label: string;
          object_type: string;
          required: boolean;
          updated_at: string;
          updated_by: string | null;
        };
        Insert: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          default_value?: Json | null;
          field_key: string;
          field_type: string;
          id?: string;
          label: string;
          object_type: string;
          required?: boolean;
          updated_at?: string;
          updated_by?: string | null;
        };
        Update: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          default_value?: Json | null;
          field_key?: string;
          field_type?: string;
          id?: string;
          label?: string;
          object_type?: string;
          required?: boolean;
          updated_at?: string;
          updated_by?: string | null;
        };
        Relationships: [];
      };
      crm_field_options: {
        Row: {
          archived_at: string | null;
          created_at: string;
          created_by: string | null;
          display_order: number;
          field_definition_id: string;
          id: string;
          label: string;
          option_key: string;
          updated_at: string;
          updated_by: string | null;
        };
        Insert: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          display_order?: number;
          field_definition_id: string;
          id?: string;
          label: string;
          option_key: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Update: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          display_order?: number;
          field_definition_id?: string;
          id?: string;
          label?: string;
          option_key?: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Relationships: [];
      };
      crm_layout_fields: {
        Row: {
          archived_at: string | null;
          created_at: string;
          created_by: string | null;
          display_order: number;
          field_definition_id: string;
          id: string;
          section_id: string;
          updated_at: string;
          updated_by: string | null;
        };
        Insert: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          display_order?: number;
          field_definition_id: string;
          id?: string;
          section_id: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Update: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          display_order?: number;
          field_definition_id?: string;
          id?: string;
          section_id?: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Relationships: [];
      };
      crm_layout_sections: {
        Row: {
          archived_at: string | null;
          created_at: string;
          created_by: string | null;
          display_order: number;
          id: string;
          object_type: string;
          section_name: string;
          updated_at: string;
          updated_by: string | null;
        };
        Insert: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          display_order?: number;
          id?: string;
          object_type: string;
          section_name: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Update: {
          archived_at?: string | null;
          created_at?: string;
          created_by?: string | null;
          display_order?: number;
          id?: string;
          object_type?: string;
          section_name?: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Relationships: [];
      };
      crm_tasks: {
        Row: {
          archived_at: string | null;
          assignee_id: string | null;
          body: string | null;
          company_id: string | null;
          contact_id: string | null;
          created_at: string;
          created_by: string | null;
          deal_id: string | null;
          due_at: string | null;
          id: string;
          status: string;
          title: string;
          updated_at: string;
          updated_by: string | null;
        };
        Insert: {
          archived_at?: string | null;
          assignee_id?: string | null;
          body?: string | null;
          company_id?: string | null;
          contact_id?: string | null;
          created_at?: string;
          created_by?: string | null;
          deal_id?: string | null;
          due_at?: string | null;
          id?: string;
          status?: string;
          title: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Update: {
          archived_at?: string | null;
          assignee_id?: string | null;
          body?: string | null;
          company_id?: string | null;
          contact_id?: string | null;
          created_at?: string;
          created_by?: string | null;
          deal_id?: string | null;
          due_at?: string | null;
          id?: string;
          status?: string;
          title?: string;
          updated_at?: string;
          updated_by?: string | null;
        };
        Relationships: [];
      };
    };
    Views: {
      [_ in never]: never;
    };
    Functions: {
      [_ in never]: never;
    };
    Enums: {
      [_ in never]: never;
    };
    CompositeTypes: {
      [_ in never]: never;
    };
  };
};

// Convenience types derived from Database
export type Activity = Database["public"]["Tables"]["crm_activities"]["Row"];
export type AuditEvent = Database["public"]["Tables"]["crm_audit_events"]["Row"];
export type Company = Database["public"]["Tables"]["crm_companies"]["Row"];
export type Contact = Database["public"]["Tables"]["crm_contacts"]["Row"];
export type Deal = Database["public"]["Tables"]["crm_deals"]["Row"];
export type DealStage = Database["public"]["Tables"]["crm_deal_stages"]["Row"];
export type Task = Database["public"]["Tables"]["crm_tasks"]["Row"];

// Pagination types
export type PaginationParams = {
  page?: number;
  pageSize?: number;
};

export type ResolvedPagination = {
  page: number;
  pageSize: number;
  from: number;
  to: number;
};

// Repository result types
export type RepoError = {
  message: string;
  code?: string;
  details?: string;
  hint?: string;
  source: string;
};

export type RepoResult<T> = {
  data: T | null;
  error: RepoError | null;
};

export type RepoListResponse<T> = {
  records: T[];
  total: number | null;
  page: number;
  pageSize: number;
};

export type RepoListResult<T> = {
  data: RepoListResponse<T> | null;
  error: RepoError | null;
};
