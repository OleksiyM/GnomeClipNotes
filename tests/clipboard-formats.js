import {textFormats} from '../extension/clipboardFormats.js';

function equal(actual, expected) {
    if (JSON.stringify(actual) !== JSON.stringify(expected))
        throw new Error(`Expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
}
equal(textFormats(['SAVE_TARGETS', 'TARGETS', 'text/html', 'text/plain;charset=UTF-8',
    'text/plain;charset=utf-8', 'UTF8_STRING']),
['text/plain;charset=utf-8', 'text/plain;charset=UTF-8', 'UTF8_STRING']);
equal(textFormats(['UTF8_STRING', 'text/plain', 'text/plain;charset=UTF-8', 'text/plain']),
    ['text/plain;charset=UTF-8', 'text/plain', 'UTF8_STRING']);
equal(textFormats(['text/html', 'image/png', 'text/plain;charset=iso-8859-1', 'STRING', null]), []);
equal(textFormats(['text/plain;charset="utf-8', 'text/plain;charset=utf-8"']), []);
equal(textFormats(['text/plain; charset="UTF-8"', 'TEXT/PLAIN']),
    ['text/plain; charset="UTF-8"', 'TEXT/PLAIN']);
equal(textFormats([]), []);
const manyAliases = Array.from({length: 32}, (_, i) => `text/plain;${' '.repeat(i)}charset=UTF-8`);
const bounded = textFormats([...manyAliases, 'text/plain;charset=utf-8']);
equal(bounded.length, 8);
equal(bounded[0], 'text/plain;charset=utf-8');
print('PASS clipboard format preference, exact advertised spelling, deduplication and text-only allowlist');
