-- Exponential Project Management Tables

-- Projects table
CREATE TABLE IF NOT EXISTS exponential_projects (
    id uuid DEFAULT gen_random_uuid() PRIMARY KEY,
    name text NOT NULL,
    description text,
    status text DEFAULT 'planning' CHECK (status IN ('planning', 'active', 'on_hold', 'completed')),
    priority text DEFAULT 'medium' CHECK (priority IN ('low', 'medium', 'high', 'critical')),
    start_date timestamptz,
    target_end_date timestamptz,
    labels text[] DEFAULT '{}',
    created_at timestamptz DEFAULT now(),
    updated_at timestamptz DEFAULT now()
);

-- Sprints table
CREATE TABLE IF NOT EXISTS exponential_sprints (
    id uuid DEFAULT gen_random_uuid() PRIMARY KEY,
    project_id uuid REFERENCES exponential_projects(id) ON DELETE CASCADE,
    name text NOT NULL,
    goal text,
    duration text,
    start_date timestamptz,
    end_date timestamptz,
    status text DEFAULT 'planned' CHECK (status IN ('planned', 'active', 'completed')),
    created_at timestamptz DEFAULT now(),
    updated_at timestamptz DEFAULT now()
);

-- Tasks table
CREATE TABLE IF NOT EXISTS exponential_tasks (
    id uuid DEFAULT gen_random_uuid() PRIMARY KEY,
    sprint_id uuid REFERENCES exponential_sprints(id) ON DELETE CASCADE,
    title text NOT NULL,
    description text,
    priority text DEFAULT 'medium' CHECK (priority IN ('low', 'medium', 'high', 'critical')),
    status text DEFAULT 'todo' CHECK (status IN ('todo', 'in_progress', 'review', 'done')),
    acceptance_criteria text[] DEFAULT '{}',
    estimated_hours numeric,
    actual_hours numeric,
    assignee_id uuid,
    created_at timestamptz DEFAULT now(),
    updated_at timestamptz DEFAULT now()
);

-- Enable RLS
ALTER TABLE exponential_projects ENABLE ROW LEVEL SECURITY;
ALTER TABLE exponential_sprints ENABLE ROW LEVEL SECURITY;
ALTER TABLE exponential_tasks ENABLE ROW LEVEL SECURITY;

-- Create policies (allow all for now, can be restricted later)
CREATE POLICY "Allow all" ON exponential_projects FOR ALL USING (true);
CREATE POLICY "Allow all" ON exponential_sprints FOR ALL USING (true);
CREATE POLICY "Allow all" ON exponential_tasks FOR ALL USING (true);
