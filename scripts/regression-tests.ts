/**
 * Regression Tests for MCP Firewall Dashboard
 * 
 * These tests verify core functionality works after each deployment
 */

interface TestResult {
  name: string;
  passed: boolean;
  duration: number;
  error?: string;
}

// Test configuration
const BASE_URL = process.env.VERCEL_URL 
  ? `https://${process.env.VERCEL_URL}` 
  : 'http://localhost:3000';

const TEST_TIMEOUT = 10000;

// Helper function for API calls
async function apiCall(endpoint: string, options?: RequestInit): Promise<Response> {
  const url = `${BASE_URL}${endpoint}`;
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), TEST_TIMEOUT);
  
  try {
    const response = await fetch(url, {
      ...options,
      signal: controller.signal,
    });
    clearTimeout(timeout);
    return response;
  } catch (error) {
    clearTimeout(timeout);
    throw error;
  }
}

// ============================================
// TEST SUITE
// ============================================

async function testHealthEndpoint(): Promise<TestResult> {
  const start = Date.now();
  try {
    const response = await apiCall('/api/health');
    if (!response.ok) {
      throw new Error(`Health check failed: ${response.status}`);
    }
    const data = await response.json();
    if (!data.status || data.status !== 'ok') {
      throw new Error('Health check returned unexpected status');
    }
    return { name: 'Health Endpoint', passed: true, duration: Date.now() - start };
  } catch (error: any) {
    return { 
      name: 'Health Endpoint', 
      passed: false, 
      duration: Date.now() - start,
      error: error.message 
    };
  }
}

async function testAuditAPI(): Promise<TestResult> {
  const start = Date.now();
  try {
    const response = await apiCall('/api/audit?limit=5');
    if (!response.ok) {
      throw new Error(`Audit API failed: ${response.status}`);
    }
    const data = await response.json();
    
    // Validate response structure
    if (!Array.isArray(data.logs)) {
      throw new Error('Audit API: logs should be an array');
    }
    if (!data.stats || typeof data.stats !== 'object') {
      throw new Error('Audit API: stats should be an object');
    }
    if (typeof data.stats.total !== 'number') {
      throw new Error('Audit API: stats.total should be a number');
    }
    
    return { name: 'Audit API', passed: true, duration: Date.now() - start };
  } catch (error: any) {
    return { 
      name: 'Audit API', 
      passed: false, 
      duration: Date.now() - start,
      error: error.message 
    };
  }
}

async function testSessionsAPI(): Promise<TestResult> {
  const start = Date.now();
  try {
    const response = await apiCall('/api/sessions');
    if (!response.ok) {
      throw new Error(`Sessions API failed: ${response.status}`);
    }
    const data = await response.json();
    
    if (!Array.isArray(data.sessions)) {
      throw new Error('Sessions API: sessions should be an array');
    }
    if (!data.stats || typeof data.stats !== 'object') {
      throw new Error('Sessions API: stats should be an object');
    }
    
    return { name: 'Sessions API', passed: true, duration: Date.now() - start };
  } catch (error: any) {
    return { 
      name: 'Sessions API', 
      passed: false, 
      duration: Date.now() - start,
      error: error.message 
    };
  }
}

async function testFirewallAPI(): Promise<TestResult> {
  const start = Date.now();
  try {
    const response = await apiCall('/api/firewall');
    if (!response.ok) {
      throw new Error(`Firewall API failed: ${response.status}`);
    }
    const data = await response.json();
    
    if (!data.config || typeof data.config !== 'object') {
      throw new Error('Firewall API: config should be an object');
    }
    if (!data.config.toolPermissions) {
      throw new Error('Firewall API: toolPermissions should exist');
    }
    
    return { name: 'Firewall API', passed: true, duration: Date.now() - start };
  } catch (error: any) {
    return { 
      name: 'Firewall API', 
      passed: false, 
      duration: Date.now() - start,
      error: error.message 
    };
  }
}

async function testAuditAPIFiltering(): Promise<TestResult> {
  const start = Date.now();
  try {
    // Test status filter
    const response = await apiCall('/api/audit?status=blocked&limit=1');
    if (!response.ok) {
      throw new Error(`Audit API filtering failed: ${response.status}`);
    }
    const data = await response.json();
    
    // All returned logs should have status 'blocked' or 'error' (mapped from blocked)
    const invalidLogs = data.logs.filter((log: any) => 
      log.status !== 'blocked' && log.status !== 'error'
    );
    if (invalidLogs.length > 0) {
      throw new Error('Audit API filtering: returned non-blocked logs');
    }
    
    return { name: 'Audit API Filtering', passed: true, duration: Date.now() - start };
  } catch (error: any) {
    return { 
      name: 'Audit API Filtering', 
      passed: false, 
      duration: Date.now() - start,
      error: error.message 
    };
  }
}

async function testCORSHeaders(): Promise<TestResult> {
  const start = Date.now();
  try {
    const response = await apiCall('/api/audit?limit=1', {
      method: 'OPTIONS',
      headers: {
        'Origin': 'https://example.com',
        'Access-Control-Request-Method': 'GET',
      },
    });
    
    const allowOrigin = response.headers.get('access-control-allow-origin');
    if (!allowOrigin) {
      throw new Error('CORS: Missing Access-Control-Allow-Origin header');
    }
    
    return { name: 'CORS Headers', passed: true, duration: Date.now() - start };
  } catch (error: any) {
    return { 
      name: 'CORS Headers', 
      passed: false, 
      duration: Date.now() - start,
      error: error.message 
    };
  }
}

async function testResponseTime(): Promise<TestResult> {
  const start = Date.now();
  try {
    const response = await apiCall('/api/audit?limit=10');
    const duration = Date.now() - start;
    
    if (duration > 2000) {
      throw new Error(`API response too slow: ${duration}ms (max 2000ms)`);
    }
    
    return { name: 'Response Time (<2s)', passed: true, duration };
  } catch (error: any) {
    return { 
      name: 'Response Time (<2s)', 
      passed: false, 
      duration: Date.now() - start,
      error: error.message 
    };
  }
}

async function testAdminPagesLoad(): Promise<TestResult> {
  const start = Date.now();
  try {
    const pages = ['/admin/firewall', '/admin/audit'];
    
    for (const page of pages) {
      const response = await apiCall(page);
      if (!response.ok && response.status !== 307 && response.status !== 302) {
        // 307/302 are OK - likely redirect to sign-in
        throw new Error(`Page ${page} failed: ${response.status}`);
      }
    }
    
    return { name: 'Admin Pages Load', passed: true, duration: Date.now() - start };
  } catch (error: any) {
    return { 
      name: 'Admin Pages Load', 
      passed: false, 
      duration: Date.now() - start,
      error: error.message 
    };
  }
}

// ============================================
// MAIN RUNNER
// ============================================

async function runTests(): Promise<void> {
  console.log('🧪 Running Regression Tests...\n');
  console.log(`Base URL: ${BASE_URL}\n`);
  
  const tests = [
    testHealthEndpoint,
    testAuditAPI,
    testSessionsAPI,
    testFirewallAPI,
    testAuditAPIFiltering,
    testCORSHeaders,
    testResponseTime,
    testAdminPagesLoad,
  ];
  
  const results: TestResult[] = [];
  
  for (const test of tests) {
    const result = await test();
    results.push(result);
    
    const icon = result.passed ? '✅' : '❌';
    const status = result.passed ? 'PASS' : 'FAIL';
    console.log(`${icon} ${result.name}: ${status} (${result.duration}ms)`);
    
    if (result.error) {
      console.log(`   Error: ${result.error}`);
    }
  }
  
  // Summary
  console.log('\n' + '='.repeat(50));
  const passed = results.filter(r => r.passed).length;
  const failed = results.filter(r => !r.passed).length;
  const totalDuration = results.reduce((sum, r) => sum + r.duration, 0);
  
  console.log(`Total: ${results.length} tests`);
  console.log(`✅ Passed: ${passed}`);
  console.log(`❌ Failed: ${failed}`);
  console.log(`⏱️  Total Duration: ${totalDuration}ms`);
  console.log('='.repeat(50));
  
  // Exit with error code if any tests failed
  if (failed > 0) {
    console.log('\n❌ REGRESSION TESTS FAILED');
    process.exit(1);
  } else {
    console.log('\n✅ ALL REGRESSION TESTS PASSED');
    process.exit(0);
  }
}

// Run tests
runTests().catch(error => {
  console.error('Test runner error:', error);
  process.exit(1);
});
