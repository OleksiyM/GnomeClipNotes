import {gettext as _, trf, setCatalog, resetCatalog} from '../extension/i18n.js';

function assert(value, message) { if (!value) throw new Error(message); }
resetCatalog();
assert(_('New Note') === 'New Note', 'English fallback before service is ready');
const catalog = {language: 'test', messages: {'New Note': '[New Note]', 'Paused until {time}': '{time}: Paused'}};
assert(setCatalog(catalog), 'First catalog accepted');
assert(!setCatalog(catalog), 'Unchanged catalog does not rebuild UI');
assert(_('New Note') === '[New Note]', 'Private catalog used');
assert(_('User content') === 'User content', 'Unknown message falls back');
assert(trf('Paused until {time}', {time: '{time}'}) === '{time}: Paused', 'Values substituted once');
assert(!setCatalog(null), 'Missing metadata does not erase last language');
assert(_('New Note') === '[New Note]', 'Retained while service stopped');
assert(setCatalog({language: 'en', messages: {'New Note': 'New Note'}}), 'English switch applied');
assert(_('New Note') === 'New Note', 'English ignores Shell locale');
resetCatalog();
assert(_('toString') === 'toString', 'No inherited object keys');
print('PASS private Shell catalog, English fallback, substitution and restart idempotence');
