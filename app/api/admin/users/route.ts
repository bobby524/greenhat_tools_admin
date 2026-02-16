// Admin Users API - List all users for the organization
import { NextRequest, NextResponse } from 'next/server';
import { createClient } from '@supabase/supabase-js';

const ORG_ID = "cd861b76-f85c-4afc-b3e8-8f85945c3132";

function getSupabase() {
  const url = process.env.SUPABASE_URL || process.env.NEXT_PUBLIC_SUPABASE_URL;
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY;
  
  if (!url || !key) {
    throw new Error('Missing Supabase environment variables');
  }
  
  return createClient(url, key);
}

// GET /api/admin/users - List all users for an organization
export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url);
    const orgId = searchParams.get('org_id') || ORG_ID;
    const search = searchParams.get('search');
    const notInTeam = searchParams.get('not_in_team');

    const supabase = getSupabase();

    // Build org users from team memberships (org_memberships table does not exist in this DB)
    const { data: memberships, error: membershipsError } = await supabase
      .from('team_members')
      .select('user_id, team_id, role, created_at, teams!inner(org_id, name)')
      .eq('teams.org_id', orgId);

    if (membershipsError) {
      console.error('Error fetching team memberships:', membershipsError);
      return NextResponse.json(
        { error: membershipsError.message },
        { status: 500 }
      );
    }

    const teamMemberships = memberships || [];
    const userIds = [...new Set(teamMemberships.map((m: any) => m.user_id))];

    const { data: usersRaw, error: usersError } = userIds.length
      ? await supabase
          .from('users')
          .select('id, first_name, last_name, email, avatar_url, created_at')
          .in('id', userIds)
      : { data: [], error: null as any };

    if (usersError) {
      console.error('Error fetching users:', usersError);
      return NextResponse.json({ error: usersError.message }, { status: 500 });
    }

    const roleRank: Record<string, number> = { member: 1, manager: 2, admin: 3 };

    // Transform and aggregate by user
    let users = (usersRaw || []).map((u: any) => {
      const userTeams = teamMemberships.filter((tm: any) => tm.user_id === u.id);
      const topRole = userTeams.reduce((best: string, tm: any) => {
        const r = tm.role || 'member';
        return (roleRank[r] || 0) > (roleRank[best] || 0) ? r : best;
      }, 'member');

      return {
        id: u.id,
        first_name: u.first_name,
        last_name: u.last_name,
        email: u.email,
        avatar_url: u.avatar_url,
        org_role: topRole,
        joined_at: u.created_at,
        teams: userTeams.map((tm: any) => ({
          team_id: tm.team_id,
          team_name: tm.teams?.name,
          role: tm.role,
        })),
      };
    });

    // Filter by search term
    if (search) {
      const searchLower = search.toLowerCase();
      users = users.filter(u => 
        u.first_name?.toLowerCase().includes(searchLower) ||
        u.last_name?.toLowerCase().includes(searchLower) ||
        u.email?.toLowerCase().includes(searchLower)
      );
    }

    // Filter by not_in_team
    if (notInTeam) {
      users = users.filter(u => 
        !u.teams.some((t: any) => t.team_id === notInTeam)
      );
    }

    return NextResponse.json({ users });
  } catch (err: any) {
    console.error('Error in admin users API:', err);
    return NextResponse.json(
      { error: err.message || 'Failed to fetch users' },
      { status: 500 }
    );
  }
}
