import { describe, it, expect } from 'vitest';
import {
  resolvePagination,
  toRepoError,
  buildRepoResult,
  buildRepoListResult,
  defaultPagination,
} from '@/app/admin/greenspot/data/repoUtils';
import type { PaginationParams, PostgrestError } from '@/app/admin/greenspot/data/types';

describe('Repository Utilities', () => {
  describe('resolvePagination', () => {
    it('should use default values when no pagination provided', () => {
      const result = resolvePagination();
      
      expect(result.page).toBe(1);
      expect(result.pageSize).toBe(25);
      expect(result.from).toBe(0);
      expect(result.to).toBe(24);
    });

    it('should use provided values', () => {
      const params: PaginationParams = { page: 3, pageSize: 10 };
      const result = resolvePagination(params);
      
      expect(result.page).toBe(3);
      expect(result.pageSize).toBe(10);
      expect(result.from).toBe(20); // (3-1) * 10
      expect(result.to).toBe(29);  // 20 + 10 - 1
    });

    it('should handle zero and negative values gracefully', () => {
      const params: PaginationParams = { page: 0, pageSize: -5 };
      const result = resolvePagination(params);
      
      expect(result.page).toBe(1); // Defaults to 1
      expect(result.pageSize).toBe(25); // Defaults to 25
    });
  });

  describe('toRepoError', () => {
    it('should return null for null error', () => {
      const result = toRepoError(null, 'test-source');
      expect(result).toBeNull();
    });

    it('should convert PostgrestError to RepoError', () => {
      const pgError: PostgrestError = {
        message: 'Connection failed',
        code: 'P0001',
        details: 'Connection timeout',
        hint: 'Check your network',
      };
      
      const result = toRepoError(pgError, 'test-source');
      
      expect(result).toEqual({
        message: 'Connection failed',
        code: 'P0001',
        details: 'Connection timeout',
        hint: 'Check your network',
        source: 'test-source',
      });
    });
  });

  describe('buildRepoResult', () => {
    it('should return data when no error', () => {
      const data = { id: '1', name: 'Test' };
      const result = buildRepoResult(data, null, 'test-source');
      
      expect(result.data).toEqual(data);
      expect(result.error).toBeNull();
    });

    it('should return null data when error exists', () => {
      const pgError: PostgrestError = {
        message: 'Not found',
        code: '404',
        details: '',
        hint: '',
      };
      
      const result = buildRepoResult({ id: '1' }, pgError, 'test-source');
      
      expect(result.data).toBeNull();
      expect(result.error).not.toBeNull();
      expect(result.error?.message).toBe('Not found');
    });
  });

  describe('buildRepoListResult', () => {
    it('should return list response when no error', () => {
      const records = [{ id: '1' }, { id: '2' }];
      const pagination = resolvePagination({ page: 1, pageSize: 10 });
      
      const result = buildRepoListResult(
        records,
        100,
        pagination,
        null,
        'test-source'
      );
      
      expect(result.data).toEqual({
        records,
        total: 100,
        page: 1,
        pageSize: 10,
      });
      expect(result.error).toBeNull();
    });

    it('should return null data when error exists', () => {
      const pgError: PostgrestError = {
        message: 'Query failed',
        code: '500',
        details: '',
        hint: '',
      };
      const pagination = resolvePagination();
      
      const result = buildRepoListResult(
        [{ id: '1' }],
        1,
        pagination,
        pgError,
        'test-source'
      );
      
      expect(result.data).toBeNull();
      expect(result.error).not.toBeNull();
    });

    it('should handle null records', () => {
      const pagination = resolvePagination();
      
      const result = buildRepoListResult(
        null,
        0,
        pagination,
        null,
        'test-source'
      );
      
      expect(result.data?.records).toEqual([]);
    });
  });

  describe('defaultPagination', () => {
    it('should return default pagination', () => {
      const result = defaultPagination();
      
      expect(result.page).toBe(1);
      expect(result.pageSize).toBe(25);
      expect(result.from).toBe(0);
      expect(result.to).toBe(24);
    });
  });
});
