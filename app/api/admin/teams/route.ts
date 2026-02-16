// Admin Teams API - List/Create teams
import { NextRequest, NextResponse } from 'next/server';
import { createClient } from '@supabase/supabase-js';

const ORG_ID = "cd861b76-f85c-4afc-b3e8-8f85945c3132";

// Lazy initialization of Supabase client
function getSupabase() {
  const url = process.env.SUPABASE_URL || process.env.NEXT_PUBLIC_SUPABASE_URL;
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY;
  
  if (!url || !key) {
    throw new Error('Missing Supabase environment variables');
  }
  
  return createClient(url, key);
}

// GET /api/admin/teams - List all teams with member counts
export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url);
    const orgId = searchParams.get('org_id') || ORG_ID;

    const supabase = getSupabase();

    // Fetch teams with member and project counts
    const { data: teams, error } = await supabase
      .from('teams')
      .select(`
        id, 
        name, 
        slug, 
        color, 
        org_id,
        created_at,
        team_members(count),
        projects(count)
      `)
      .eq('org_id', orgId)
      .order('name', { ascending: true });

    if (error) {
      console.error('Error fetching teams:', error);
      return NextResponse.json(
        { error: error.message },
        { status: 500 }
      );
    }

    // Transform count results
    const transformedTeams = (teams || []).map((team: any) => ({
      ...team,
      member_count: team.team_members?.[0]?.count || 0,
      project_count: team.projects?.[0]?.count || 0,
    }));

    return NextResponse.json({ teams: transformedTeams });
  } catch (err: any) {
    console.error('Error in admin teams API:', err);
    return NextResponse.json(
      { error: err.message || 'Failed to fetch teams' },
      { status: 500 }
    );
  }
}

// POST /api/admin/teams - Create a new team
export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const { name, slug, color, org_id } = body;

    if (!name || !org_id) {
      return NextResponse.json(
        { error: 'name and org_id are required' },
        { status: 400 }
      );
    }

    const supabase = getSupabase();

    // Generate slug if not provided
    const teamSlug = slug || name.toLowerCase().replace(/[^a-z0-9]+/g, '-');

    const { data, error } = await supabase
      .from('teams')
      .insert({
        name,
        slug: teamSlug,
        color: color || '#62ac4a',
        org_id,
      })
      .select()
      .single();

    if (error) {
      console.error('Error creating team:', error);
      return NextResponse.json(
        { error: error.message },
        { status: 500 }
      );
    }

    return NextResponse.json({ team: data }, { status: 201 });
  } catch (err: any) {
    console.error('Error creating team:', err);
    return NextResponse.json(
      { error: err.message || 'Failed to create team' },
      { status: 500 }
    );
  }
}
