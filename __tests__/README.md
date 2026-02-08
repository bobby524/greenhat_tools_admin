# Test Suite for Greenhat Admin

This test suite provides comprehensive testing for the Greenhat Admin application using Vitest.

## Test Structure

```
__tests__/
├── setup.ts                      # Test environment setup
├── lib/
│   ├── db.test.ts               # Database utilities tests
│   └── email.test.ts            # Email service tests
├── api/
│   ├── users.test.ts            # Users API tests
│   ├── invites.test.ts          # Invites API tests
│   └── health.test.ts           # Health check API tests
├── data/
│   ├── customization.test.ts    # Customization data utilities
│   └── repoUtils.test.ts        # Repository utilities tests
└── components/
    └── AdminLayout.test.tsx     # AdminLayout component tests
```

## Running Tests

### Run all tests
```bash
npm test
```

### Run tests in watch mode (for development)
```bash
npm run test:watch
```

### Run tests with coverage
```bash
npx vitest run --coverage
```

### Run specific test file
```bash
npx vitest run __tests__/lib/db.test.ts
```

## Test Categories

### Unit Tests
- **Database utilities** (`lib/db.test.ts`): Tests for connection pooling, query execution
- **Email service** (`lib/email.test.ts`): Tests for invite email sending
- **Data utilities** (`data/*.test.ts`): Tests for field key normalization, localStorage operations

### API Tests
- **Users API** (`api/users.test.ts`): GET and PATCH endpoints for user management
- **Invites API** (`api/invites.test.ts`): Full CRUD operations for invitations
- **Health API** (`api/health.test.ts`): Health check endpoint

### Component Tests
- **AdminLayout** (`components/AdminLayout.test.tsx`): Authentication state, navigation, access control

## Mocking Strategy

### External Dependencies
- **pg (PostgreSQL)**: Mocked to avoid actual database connections
- **resend**: Mocked to avoid sending real emails
- **next/navigation**: Mocked router and pathname
- **next/headers**: Mocked cookies and headers

### Local Dependencies
- **@/lib/db**: Mocked in API tests to control database responses
- **@/lib/email**: Mocked in invite tests
- **@/lib/auth-client**: Mocked in component tests

## Writing New Tests

### Example: Testing a new API route

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { NextRequest } from 'next/server';
import { GET } from '@/app/api/my-new-route/route';

vi.mock('@/lib/db', () => ({
  getPool: vi.fn(),
}));

describe('My New API', () => {
  it('should return expected data', async () => {
    const request = new NextRequest('http://localhost:3000/api/my-new-route');
    const response = await GET(request);
    
    expect(response.status).toBe(200);
    const data = await response.json();
    expect(data).toEqual({ success: true });
  });
});
```

### Example: Testing a React component

```typescript
import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import MyComponent from '@/app/components/MyComponent';

describe('MyComponent', () => {
  it('should render correctly', () => {
    render(<MyComponent title="Test" />);
    expect(screen.getByText('Test')).toBeInTheDocument();
  });
});
```

## Environment Variables

Tests run with the following defaults:
- `NODE_ENV=test`
- `RESEND_API_KEY=test-key`
- `RESEND_FROM_EMAIL=test@example.com`
- `BETTER_AUTH_URL=https://admin.example.com`

You can override these in `__tests__/setup.ts` or individual test files.

## Coverage Report

After running tests with coverage, open `coverage/index.html` to view the detailed coverage report.

## CI/CD Integration

Tests are configured to run in CI environments:
```yaml
# Example GitHub Actions
- name: Run tests
  run: npm test
```

## Troubleshooting

### "Cannot find module" errors
Make sure path aliases are correctly configured in `vitest.config.ts`:
```typescript
resolve: {
  alias: {
    '@': path.resolve(__dirname, '.'),
  },
}
```

### "window is not defined" errors
Use the jsdom environment (configured in vitest.config.ts) for DOM-related tests.

### Database connection errors
Ensure `pg` is properly mocked before importing modules that use it.
