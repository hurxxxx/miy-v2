import { describe, expect, it, vi } from 'vitest';

import {
  getKoreanHolidayNames,
  getKoreanHolidaysOfYear,
  getKoreanHolidayYearRange,
} from './korean-holidays';

describe('korean holidays', () => {
  it('includes the 2027 holiday dataset', () => {
    expect(getKoreanHolidayNames(2027, 0, 1)).toEqual(['1월 1일']);
    expect(getKoreanHolidayNames(2027, 1, 9)).toEqual([
      '대체공휴일(설날)',
    ]);
  });

  it('does not warn for supported 2027 lookups', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);

    getKoreanHolidayNames(2027, 4, 5);

    expect(warn).not.toHaveBeenCalled();
    warn.mockRestore();
  });
});

describe('korean holidays of a year', () => {
  it('reports the years the bundled dataset covers', () => {
    expect(getKoreanHolidayYearRange()).toEqual({ min: 2018, max: 2027 });
  });

  it('has a list for every year in the range and none outside it', () => {
    const { min, max } = getKoreanHolidayYearRange();

    for (let year = min; year <= max; year += 1) {
      expect(getKoreanHolidaysOfYear(year), String(year)).not.toBeNull();
    }
    expect(getKoreanHolidaysOfYear(min - 1)).toBeNull();
    expect(getKoreanHolidaysOfYear(max + 1)).toBeNull();
    expect(getKoreanHolidaysOfYear(2000)).toBeNull();
    expect(getKoreanHolidaysOfYear(2026.5)).toBeNull();
    expect(getKoreanHolidaysOfYear(Number.NaN)).toBeNull();
  });

  it('maps each date of a year to its names', () => {
    const year = getKoreanHolidaysOfYear(2026);

    expect(Object.keys(year ?? {})).toHaveLength(22);
    expect(year?.['2026-01-01']).toEqual(['1월 1일']);
    expect(year?.['2026-10-05']).toEqual(['대체공휴일(개천절)']);
    expect(year?.['2026-10-09']).toEqual(['한글날']);
    // 평일·일반 주말은 휴일이 아니다.
    expect(year?.['2026-10-06']).toBeUndefined();
  });

  it('keeps every name when one date carries more than one', () => {
    expect(getKoreanHolidaysOfYear(2025)?.['2025-05-05']).toEqual([
      '어린이날',
      '부처님 오신 날',
    ]);
  });

  it('lists only dates of its own year, in date order, and agrees with the date lookup', () => {
    const { min, max } = getKoreanHolidayYearRange();

    for (let year = min; year <= max; year += 1) {
      const holidays = getKoreanHolidaysOfYear(year) ?? {};
      const dates = Object.keys(holidays);

      expect(dates.length, String(year)).toBeGreaterThan(0);
      expect(dates, String(year)).toEqual([...dates].sort());
      for (const date of dates) {
        expect(date, String(year)).toMatch(/^\d{4}-\d{2}-\d{2}$/);
        expect(date.startsWith(`${year}-`), date).toBe(true);
        const names = holidays[date];
        expect(names.length, date).toBeGreaterThan(0);
        for (const name of names) expect(name.trim(), date).not.toBe('');
        const [, month, day] = date.split('-').map(Number);
        expect(getKoreanHolidayNames(year, month - 1, day), date).toEqual(
          names,
        );
      }
    }
  });

  it('hands out a copy that cannot change the bundled data', () => {
    const first = getKoreanHolidaysOfYear(2026) as Record<string, string[]>;

    first['2026-10-09'].push('덧붙인 이름');
    delete first['2026-01-01'];

    expect(getKoreanHolidaysOfYear(2026)?.['2026-10-09']).toEqual(['한글날']);
    expect(getKoreanHolidaysOfYear(2026)?.['2026-01-01']).toEqual(['1월 1일']);
    expect(getKoreanHolidayNames(2026, 9, 9)).toEqual(['한글날']);
  });

  it('does not warn when asked about a year it does not cover', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);

    expect(getKoreanHolidaysOfYear(2099)).toBeNull();
    getKoreanHolidayYearRange();

    expect(warn).not.toHaveBeenCalled();
    warn.mockRestore();
  });
});
