// Admin Team Detail API - Get/Update/Delete team
import { NextRequest, NextResponse } from 'next/server';
import { createClient } from '@supabase/supabase-js';

function getSupabase() {
  const url = process.env.SUPABASE_URL || process.env.NEXT_PUBLIC_SUPABASE_URL;
  const key = process.env.SUPABASE_SERVICE_ROLE_KEY;
  
  if (!url || !key) {
    throw new Error('Missing Supabase environment variables');
  }
  
  return createClient(url, key);
}

// GET /api/admin/teams/[id] - Get team with members
export async function GET(
  request: NextRequest,
  { params }: { params: { id: string } }
) {
  try {
    const teamId = params.id;

    if (!teamId) {
      return NextResponse.json(
        { error: 'Team ID is required' },
        { status: 400 }
      );
    }

    const supabase = getSupabase();

    // Fetch team details
    const { data: team, error: teamError } = await supabase
      .from('teams')
      .select('id, name, slug, color, org_id, created_at')
      .eq('id', teamId)
      .single();

    if (teamError) {
      if (teamError.code === 'PGRST116') {
        return NextResponse.json(
          { error: 'Team not found' },
          { status: 404 }
        );
      }
      console.error('Error fetching team:', teamError);
      return NextResponse.json(
        { error: teamError.message },
        { status: 500 }
      );
    }

    // Fetch team members with user details
    const { data: members, error: membersError } = await supabase
      .from('team_members')
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
      .eq('team_id', teamId);

    if (membersError) {
      console.error('Error fetching team members:', membersError);
    }

    // Fetch team projects
    const { data: projects, error: projectsError } = await supabase
      .from('projects')
      .select('id, name, color, state, created_at')
      .eq('team_id', teamId)
      .order('name', { ascending: true });

    if (projectsError) {
      console.error('Error fetching team projects:', projectsError);
    }

    // Transform members data
    const transformedMembers = (members || []).map((m: any) => ({
      id: m.id,
      role: m.role,
      created_at: m.created_at,
      user_id: m.user?.id,
      first_name: m.user?.first_name,
      last_name: m.user?.last_name,
      email: m.user?.email,
      avatar_url: m.user?.avatar_url,
    }));

    return NextResponse.json({
      team,
      members: transformedMembers,
      projects: projects || [],
    });
  } catch (err: any) {
    console.error('Error in team detail API:', err);
    return NextResponse.json(
      { error: err.message || 'Failed to fetch team' },
      { status: 500 }
    );
  }
}

// PATCH /api/admin/teams/[id] - Update team
export async function PATCH(
  request: NextRequest,
  { params }: { params: { id: string } }
) {
  try {
    const teamId = params.id;
    const body = await request.json();

    if (!teamId) {
      return NextResponse.json(
        { error: 'Team ID is required' },
        { status: 400 }
      );
    }

    const supabase = getSupabase();

    // Build update object with only provided fields
    const updates: any = {
      updated_at: new Date().toISOString(),
    };

    if (body.name !== undefined) updates.name = body.name;
    if (body.slug !== undefined) updates.slug = body.slug;
    if (body.color !== undefined) updates.color = body.color;

    const { data, error } = await supabase
      .from('teams')
      .update(updates)
      .eq('id', teamId)
      .select()
      .single();

    if (error) {
      console.error('Error updating team:', error);
      return NextResponse.json(
        { error: error.message },
        { status: 500 }
      );
    }

    return NextResponse.json({ team: data });
  } catch (err: any) {
    console.error('Error updating team:', err);
    return NextResponse.json(
      { error: err.message || 'Failed to update team' },
      { status: 500 }
    );
  }
}

// DELETE /api/admin/teams/[id] - Delete team
export async function DELETE(
  request: NextRequest,
  { params }: { params: { id: string } }
) {
  try {
    const teamId = params.id;

    if (!teamId) {
      return NextResponse.json(
        { error: 'Team ID is required' },
        { status: 400 }
      );
    }

    const supabase = getSupabase();

    const { error } = await supabase
      .from('teams')
      .delete()
      .eq('id', teamId);

    if (error) {
      console.error('Error deleting team:', error);
      return NextResponse.json(
        { error: error.message },
        { status: 500 }
      );
    }

    return NextResponse.json({ 
      message: 'Team deleted successfully',
      id: teamId 
    });
  } catch (err: any) {
    console.error('Error deleting team:', err);
    return NextResponse.json(
      { error: err.message || 'Failed to delete team' },
      { status: 500 }
    );
  }
}
