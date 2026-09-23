// Application-owned, domain-local catalog. Never change LANGUAGE, setlocale,
// or Shell's gettext domain: manual language selection belongs only to us.
let messages = Object.create(null);
let signature = '';

export function setCatalog(catalog) {
    if (!catalog || typeof catalog.messages !== 'object' || !catalog.messages)
        return false;
    const entries = Object.entries(catalog.messages)
        .filter(([key, value]) => key && typeof value === 'string');
    const next = JSON.stringify([catalog.language, entries]);
    if (next === signature) return false;
    messages = Object.assign(Object.create(null), Object.fromEntries(entries));
    signature = next;
    return true;
}

export function gettext(message) {
    return Object.hasOwn(messages, message) ? messages[message] : message;
}

export function trf(message, values) {
    const format = template => {
        let valid = true;
        const result = template.replace(/\{([A-Za-z0-9_]+)\}/g, (match, name) => {
            if (!Object.hasOwn(values, name)) { valid = false; return match; }
            return String(values[name]);
        });
        return valid ? result : null;
    };
    return format(gettext(message)) ?? format(message) ?? message;
}

export function resetCatalog() {
    messages = Object.create(null);
    signature = '';
}
