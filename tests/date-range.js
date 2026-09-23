import {customDateRange, localDate, formatLocalDate} from '../extension/dateRange.js';

function assert(value, message) { if (!value) throw new Error(message); }
for (const invalid of ['', '202', '2026-2-01', '2026-02-29', '2024-02-30', '2026-13-01', '0000-01-01']) {
    assert(customDateRange(invalid, '2099-01-01').error === 'invalid', `Reject ${invalid}`);
}
assert(customDateRange('2026-02-02', '2026-02-01').error === 'reversed', 'Reject reversed range');
for (const day of ['2024-02-29', '2026-03-08', '2026-11-01', '0099-12-31']) {
    assert(formatLocalDate(localDate(day)) === day, 'Calendar formatting keeps local date');
    const range = customDateRange(day, day);
    assert(!range.error, `Valid date ${day}`);
    const start = new Date(range.since * 1000);
    const end = new Date(range.until * 1000);
    assert(start.getHours() === 0 && end.getHours() === 23 && end.getMinutes() === 59 && end.getSeconds() === 59, 'Whole local day');
    assert(start.getDate() === end.getDate(), 'Inclusive final date');
    if (day === '2026-03-08' && start.getTimezoneOffset() !== end.getTimezoneOffset())
        assert(range.until - range.since + 1 === 23 * 3600, 'Spring DST day');
    if (day === '2026-11-01' && start.getTimezoneOffset() !== end.getTimezoneOffset())
        assert(range.until - range.since + 1 === 25 * 3600, 'Autumn DST day');
}
print('PASS strict custom dates, local midnight, inclusive end and DST');
