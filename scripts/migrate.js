const { Pool } = require('pg');

const fs = require('fs');
const path = require('path');

async function migrate() {
  const databaseUrl = process.env.CRM_POSTGRES_URL_NON_POOLING ||
                     process.env.crm_POSTGRES_URL_NON_POOLING ||
                     process.env.CRM_POSTGRES_URL ||
                     process.env.crm_POSTGRES_URL;
  
  if (!databaseUrl) {
    console.error('No database URL found');
    process.exit(1);
  }

  console.log('Connecting to database...');
  
  const pool = new Pool({
    connectionString: databaseUrl,
    ssl: { rejectUnauthorized: false, ca: undefined },
  });

  try {
    const sql = fs.readFileSync(
      path.join(__dirname, '..', 'supabase', 'migrations', '001_better_auth.sql'),
      'utf8'
    );

    console.log('Running migration...');
    await pool.query(sql);
    console.log('✅ Migration complete!');
  } catch (error) {
    console.error('❌ Migration failed:', error.message);
    process.exit(1);
  } finally {
    await pool.end();
  }
}

migrate();