// Preserve the exact advertised spelling when requesting a transfer. Some
// providers advertise equivalent MIME aliases but cannot serve every alias.
// Bound retries even if an owner advertises many equivalent aliases.
const MAX_TEXT_FORMATS = 8;

export function textFormats(advertised) {
    const unique = [...new Set(advertised.filter(value => typeof value === 'string'))];
    const rank = mime => {
        if (mime === 'text/plain;charset=utf-8') return 0;
        if (/^text\/plain\s*;\s*charset\s*=\s*(?:utf-8|"utf-8")\s*$/i.test(mime)) return 1;
        if (mime.toLowerCase() === 'text/plain') return 2;
        if (mime === 'UTF8_STRING') return 3;
        return 4;
    };
    return unique.filter(mime => rank(mime) < 4)
        .sort((a, b) => rank(a) - rank(b)).slice(0, MAX_TEXT_FORMATS);
}
