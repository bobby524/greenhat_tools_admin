-- Create invites table for user invitation system
CREATE TABLE IF NOT EXISTS invites (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  email VARCHAR(255) NOT NULL,
  token VARCHAR(255) UNIQUE NOT NULL,
  role VARCHAR(50) NOT NULL DEFAULT 'user',
  "invitedBy" UUID NOT NULL REFERENCES "user"(id) ON DELETE CASCADE,
  "expiresAt" TIMESTAMP WITH TIME ZONE NOT NULL,
  "usedAt" TIMESTAMP WITH TIME ZONE,
  "createdAt" TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Index for fast token lookups
CREATE INDEX IF NOT EXISTS idx_invites_token ON invites(token);

-- Index for email lookups
CREATE INDEX IF NOT EXISTS idx_invites_email ON invites(email);

-- Index for invitedBy lookups
CREATE INDEX IF NOT EXISTS idx_invites_invited_by ON invites("invitedBy");
