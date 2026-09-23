// Strict local calendar dates, never Date.parse's UTC interpretation/rollover.
export function localDate(text) {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(text)) return null;
    const [year, month, day] = text.split('-').map(Number);
    if (year < 1 || month < 1 || month > 12 || day < 1 || day > 31) return null;
    const date = new Date(0);
    date.setFullYear(year, month - 1, day);
    date.setHours(0, 0, 0, 0);
    return date.getFullYear() === year && date.getMonth() === month - 1 && date.getDate() === day ? date : null;
}

export function formatLocalDate(date) {
    return `${String(date.getFullYear()).padStart(4, '0')}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

export function customDateRange(from, to) {
    const start = localDate(from.trim());
    const end = localDate(to.trim());
    if (!start || !end) return {error: 'invalid'};
    if (start > end) return {error: 'reversed'};
    // Include the whole final local day, including 23/25-hour DST days.
    end.setDate(end.getDate() + 1);
    return {since: Math.floor(start.getTime() / 1000), until: Math.floor(end.getTime() / 1000) - 1};
}
