// Admin Team Members API - Add/Remove/Update team members
import { NextRequest, NextResponse } from 'next/server';
import { createClient } from '@supabase/supabase-js';

// Valid team member roles
const VALID_TEAM_ROLES = ['admin', 'manager', 'member'] as const;
type TeamRole = typeof VALID_TEAM_ROLES[number];

function getSupabase() {
  const url = process.env.SUPABASE_URL || process.env.NEXT_PUBLIC_SUPABASE_URL;
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY;
  
  if (!url || !key) {
    throw new Error('Missing Supabase environment variables');
  }
  
  return createClient(url, key);
}

// POST /api/admin/team-members - Add a user to a team
export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const { team_id, user_id, role = 'member' } = body;

    if (!team_id || !user_id) {
      return NextResponse.json(
        { error: 'team_id and user_id are required' },
        { status: 400 }
      );
    }

    // Validate role
    if (!VALID_TEAM_ROLES.includes(role as TeamRole)) {
      return NextResponse.json(
        { error: 'role must be admin, manager, or member' },
        { status: 400 }
      );
    }

    const supabase = getSupabase();

    const { data, error } = await supabase
      .from('team_members')
      .insert({
        team_id,
        user_id,
        role,
      })
      .select(`
        id,
        role,
        created_at,
        user:user_id (
          id,
          first_name,
          last_name,
          email,
          avatar_url
        )
      `)
      .single();

    if (error) {
      if (error.code === '23505') {
        return NextResponse.json(
          { error: 'User is already a member of this team' },
          { status: 409 }
        );
      }
      console.error('Error adding team member:', error);
      return NextResponse.json(
        { error: error.message },
        { status: 500 }
      );
    }

    // Transform response
    const transformed = {
      id: data.id,
      role: data.role,
      created_at: data.created_at,
      user_id: data.user?.id,
      first_name: data.user?.first_name,
      last_name: data.user?.last_name,
      email: data.user?.email,
      avatar_url: data.user?.avatar_url,
    };

    return NextResponse.json({ member: transformed }, { status: 201 });
  } catch (err: any) {
    console.error('Error adding team member:', err);
    return NextResponse.json(
      { error: err.message || 'Failed to add team member' },
      { status: 500 }
    );
  }
}

// DELETE /api/admin/team-members - Remove a user from a team
export async function DELETE(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url);
    const teamId = searchParams.get('team_id');
    const userId = searchParams.get('user_id');
    const memberId = searchParams.get('id');

    const supabase = getSupabase();

    let query = supabase.from('team_members').delete();

    if (memberId) {
      query = query.eq('id', memberId);
    } else if (teamId && userId) {
      query = query.eq('team_id', teamId).eq('user_id', userId);
    } else {
      return NextResponse.json(
        { error: 'Either id or both team_id and user_id are required' },
        { status: 400 }
      );
    }

    const { error } = await query;

    if (error) {
      console.error('Error removing team member:', error);
      return NextResponse.json(
        { error: error.message },
        { status: 500 }
      );
    }

    return NextResponse.json({ 
      message: 'Team member removed successfully' 
    });
  } catch (err: any) {
    console.error('Error removing team member:', err);
    return NextResponse.json(
      { error: err.message || 'Failed to remove team member' },
      { status: 500 }
    );
  }
}

// PATCH /api/admin/team-members - Update a team member's role
export async function PATCH(request: NextRequest) {
  try {
    const body = await request.json();
    const { id, role } = body;

    if (!id || !role) {
      return NextResponse.json(
        { error: 'id and role are required' },
        { status: 400 }
      );
    }

    // Validate role
    if (!VALID_TEAM_ROLES.includes(role as TeamRole)) {
      return NextResponse.json(
        { error: 'role must be admin, manager, or member' },
        { status: 400 }
      );
    }

    const supabase = getSupabase();

    const { data, error } = await supabase
      .from('team_members')
      .update({ role, updated_at: new Date().toISOString() })
      .eq('id', id)
      .select(`
        id,
        role,
        created_at,
        user:user_id (
          id,
          first_name,
          last_name,
          email,
          avatar_url
        )
      `)
      .single();

    if (error) {
      console.error('Error updating team member:', error);
      return NextResponse.json(
        { error: error.message },
        { status: 500 }
      );
    }

    // Transform response
    const transformed = {
      id: data.id,
      role: data.role,
      created_at: data.created_at,
      user_id: data.user?.id,
      first_name: data.user?.first_name,
      last_name: data.user?.last_name,
      email: data.user?.email,
      avatar_url: data.user?.avatar_url,
    };

    return NextResponse.json({ member: transformed });
  } catch (err: any) {
    console.error('Error updating team member:', err);
    return NextResponse.json(
      { error: err.message || 'Failed to update team member' },
      { status: 500 }
    );
  }
}
