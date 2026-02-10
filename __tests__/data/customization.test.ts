import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock localStorage
const mockLocalStorage = {
  getItem: vi.fn(),
  setItem: vi.fn(),
  removeItem: vi.fn(),
  clear: vi.fn(),
};

Object.defineProperty(global, 'localStorage', {
  value: mockLocalStorage,
  writable: true,
});

// Mock window
Object.defineProperty(global, 'window', {
  value: { localStorage: mockLocalStorage },
  writable: true,
});

import {
  loadStoredValue,
  saveStoredValue,
  getEntityStorageKey,
  crmSettingsStoragePrefix,
  normalizeFieldKey,
  createUniqueFieldKey,
} from '@/app/admin/greenspot/data/customization';
import type { FieldDefinition } from '@/app/admin/greenspot/data/customization';

describe('Customization Data Utilities', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('loadStoredValue', () => {
    it('should return null on server-side (no window)', () => {
      const originalWindow = global.window;
      // @ts-ignore
      global.window = undefined;
      
      const result = loadStoredValue('test-key');
      
      expect(result).toBeNull();
      
      global.window = originalWindow;
    });

    it('should return null if key does not exist', () => {
      mockLocalStorage.getItem.mockReturnValue(null);
      
      const result = loadStoredValue('non-existent-key');
      
      expect(result).toBeNull();
      expect(mockLocalStorage.getItem).toHaveBeenCalledWith('non-existent-key');
    });

    it('should parse and return stored JSON value', () => {
      const storedData = { fields: [], sections: [] };
      mockLocalStorage.getItem.mockReturnValue(JSON.stringify(storedData));
      
      const result = loadStoredValue('test-key');
      
      expect(result).toEqual(storedData);
    });

    it('should return null if JSON parsing fails', () => {
      mockLocalStorage.getItem.mockReturnValue('invalid json');
      
      const result = loadStoredValue('test-key');
      
      expect(result).toBeNull();
    });
  });

  describe('saveStoredValue', () => {
    it('should do nothing on server-side (no window)', () => {
      const originalWindow = global.window;
      // @ts-ignore
      global.window = undefined;
      
      saveStoredValue('test-key', { data: true });
      
      expect(mockLocalStorage.setItem).not.toHaveBeenCalled();
      
      global.window = originalWindow;
    });

    it('should save value as JSON string', () => {
      const value = { fields: [{ id: '1', label: 'Test' }] };
      
      saveStoredValue('test-key', value);
      
      expect(mockLocalStorage.setItem).toHaveBeenCalledWith(
        'test-key',
        JSON.stringify(value)
      );
    });

    it('should handle storage errors gracefully', () => {
      mockLocalStorage.setItem.mockImplementation(() => {
        throw new Error('Storage quota exceeded');
      });
      
      // Should not throw
      expect(() => saveStoredValue('test-key', { data: true })).not.toThrow();
    });
  });

  describe('getEntityStorageKey', () => {
    it('should generate correct storage key with prefix', () => {
      const key = getEntityStorageKey('contacts');
      
      expect(key).toBe(`${crmSettingsStoragePrefix}-entity-contacts`);
    });

    it('should handle different entity types', () => {
      expect(getEntityStorageKey('companies')).toContain('companies');
      expect(getEntityStorageKey('deals')).toContain('deals');
      expect(getEntityStorageKey('tasks')).toContain('tasks');
    });
  });

  describe('normalizeFieldKey', () => {
    it('should convert to lowercase', () => {
      expect(normalizeFieldKey('FirstName')).toBe('firstname');
    });

    it('should replace spaces with underscores', () => {
      expect(normalizeFieldKey('First Name')).toBe('first_name');
    });

    it('should replace special characters with underscores', () => {
      expect(normalizeFieldKey('Name@Company!')).toBe('name_company');
    });

    it('should trim leading and trailing underscores', () => {
      expect(normalizeFieldKey('__Test__')).toBe('test');
    });

    it('should handle multiple consecutive special characters', () => {
      expect(normalizeFieldKey('Test!!!Field')).toBe('test_field');
    });

    it('should return "field" for empty result', () => {
      expect(normalizeFieldKey('!!!')).toBe('field');
    });
  });

  describe('createUniqueFieldKey', () => {
    it('should return base key if not used', () => {
      const existingFields: FieldDefinition[] = [
        { id: '1', label: 'Other', fieldKey: 'other', type: 'text', required: false },
      ];
      
      const key = createUniqueFieldKey('First Name', existingFields);
      
      expect(key).toBe('first_name');
    });

    it('should append suffix if key already exists', () => {
      const existingFields: FieldDefinition[] = [
        { id: '1', label: 'First Name', fieldKey: 'first_name', type: 'text', required: false },
      ];
      
      const key = createUniqueFieldKey('First Name', existingFields);
      
      expect(key).toBe('first_name_2');
    });

    it('should increment suffix if multiple duplicates exist', () => {
      const existingFields: FieldDefinition[] = [
        { id: '1', label: 'First', fieldKey: 'first_name', type: 'text', required: false },
        { id: '2', label: 'First', fieldKey: 'first_name_2', type: 'text', required: false },
      ];
      
      const key = createUniqueFieldKey('First Name', existingFields);
      
      expect(key).toBe('first_name_3');
    });

    it('should be case insensitive when checking existing keys', () => {
      const existingFields: FieldDefinition[] = [
        { id: '1', label: 'First', fieldKey: 'FIRST_NAME', type: 'text', required: false },
      ];
      
      const key = createUniqueFieldKey('first_name', existingFields);
      
      expect(key).toBe('first_name_2');
    });
  });
});
