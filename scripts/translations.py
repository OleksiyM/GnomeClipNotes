#!/usr/bin/env python3
"""Reproducible gettext extraction, validation, and catalog compilation."""

import argparse
import gettext
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
PO = ROOT / "po"
DOMAIN = "gnome-clip-notes"
POT = PO / f"{DOMAIN}.pot"
SHELL_MESSAGES = PO / "shell-messages.json"

RUST_KEYWORDS = [
    "tr:1", "trf:1", "ntr:1,2", "ntrf:1,2", "mark:1", "ptr:1c,2",
    "button:1", "label:1", "action_row:1", "action_row:2",
    "switch_row:1", "switch_row:2", "combo:1",
    "confirm:2", "confirm:3", "confirm:4", "prompt:2",
]
JS_KEYWORDS = ["_:1", "trf:1", "mark:1"]
PLACEHOLDER = re.compile(r"(?<!\{)\{([A-Za-z_][A-Za-z0-9_]*)\}(?!\})|%s")


def run(*args, env=None):
    subprocess.run(args, cwd=ROOT, check=True, env=env)


def tools(required=("xgettext", "msgcat", "msgattrib", "msginit", "msgfmt", "msgfilter")):
    missing = [name for name in required
               if shutil.which(name) is None]
    if missing:
        raise SystemExit("Install GNU gettext tools: " + ", ".join(missing))


def require_rust_gettext():
    version = subprocess.run(("xgettext", "--version"), cwd=ROOT, check=True,
                             capture_output=True, text=True).stdout.splitlines()[0]
    match = re.search(r"\b(\d+)\.(\d+)(?:\.\d+)?\b", version)
    if not match or tuple(map(int, match.groups())) < (0, 24):
        raise SystemExit("Rust extraction requires GNU gettext >= 0.24 "
                         f"(found: {version})")


def source_files():
    rust = []
    for path in sorted((ROOT / "src").rglob("*.rs")):
        name = path.name
        if name == "smoke.rs" or name.endswith("_smoke.rs") or "test" in path.parts:
            continue
        rust.append(path.relative_to(ROOT).as_posix())
    js = [p.relative_to(ROOT).as_posix() for p in sorted((ROOT / "extension").glob("*.js"))]
    return rust, js


def extract(output, language, keywords, files):
    by_name = {}
    for keyword in keywords:
        by_name.setdefault(keyword.split(":", 1)[0], []).append(keyword)
    passes = [[values[0] for values in by_name.values()]]
    passes.extend([keyword] for values in by_name.values() for keyword in values[1:])
    chunks = []
    for number, pass_keywords in enumerate(passes):
        chunk = output.with_name(f"{output.stem}-{number}{output.suffix}")
        chunks.append(chunk)
        args = ["xgettext", f"--language={language}", "--from-code=UTF-8", "--keyword",
            "--force-po", "--no-wrap", "--add-location=file",
            "--add-comments=Translators:", "--package-name=GnomeClipNotes",
            "--package-version=0.2.0", "--msgid-bugs-address=https://github.com/",
            "--copyright-holder=GnomeClipNotes contributors", "--output", str(chunk)]
        for keyword in pass_keywords:
            args.append(f"--keyword={keyword}")
        if language == "Rust":
            for keyword in ("trf:1", "ntrf:1", "ntrf:2"):
                if keyword in pass_keywords:
                    args.append(f"--flag={keyword}:rust-format")
        run(*args, *files, env={**os.environ, "TZ": "UTC"})
    run("msgcat", "--use-first", "--sort-output", "--no-wrap", "--output", str(output),
        *map(str, chunks))


def english_catalog(po_file, directory):
    english_po = directory / "en.po"
    mo = directory / "en.mo"
    run("msginit", "--no-translator", "--locale=en", "--input", str(po_file),
        "--output-file", str(english_po))
    run("msgfmt", "--check", "--output-file", str(mo), str(english_po))
    with mo.open("rb") as stream:
        return gettext.GNUTranslations(stream)._catalog


def placeholders(text):
    return sorted(PLACEHOLDER.findall(text))


def validate_placeholders(po_file, template=POT):
    with tempfile.TemporaryDirectory(prefix="gcn-i18n-check-") as raw:
        tmp = Path(raw)
        source = english_catalog(template, tmp)
        mo = tmp / "translation.mo"
        run("msgfmt", "--check", "--check-format", "--output-file", str(mo), str(po_file))
        with mo.open("rb") as stream:
            translated = gettext.GNUTranslations(stream)._catalog
        for key, value in translated.items():
            if key == "":
                continue
            source_key = (key[0], 0 if key[1] == 0 else 1) if isinstance(key, tuple) else key
            source_value = source.get(source_key)
            if source_value is None:
                raise SystemExit(f"{po_file}: translated message is absent from {template}: {key!r}")
            if not value:
                continue
            if placeholders(source_value) != placeholders(value):
                raise SystemExit(f"{po_file}: placeholders differ for {key!r}: "
                                 f"{placeholders(source_value)} != {placeholders(value)}")
        plural_stems = {key[0] for key in source if isinstance(key, tuple)}
        for stem in plural_stems:
            forms = [value for key, value in source.items()
                     if isinstance(key, tuple) and key[0] == stem]
            if forms and any(placeholders(forms[0]) != placeholders(form) for form in forms[1:]):
                raise SystemExit(f"{template}: singular/plural placeholders differ for {stem!r}")


def selftest():
    tools(("msginit", "msgfmt"))
    header = '''msgid ""
msgstr ""
"Project-Id-Version: placeholder-selftest\\n"
"PO-Revision-Date: 1970-01-01 00:00+0000\\n"
"Last-Translator: Test <test@example.invalid>\\n"
"Language-Team: Test\\n"
"MIME-Version: 1.0\\n"
"Content-Type: text/plain; charset=UTF-8\\n"
"Content-Transfer-Encoding: 8bit\\n"
'''
    template_body = '''
msgid "{count} item"
msgid_plural "{count} items"
msgstr[0] ""
msgstr[1] ""
'''
    translation_body = '''
"Language: ru\\n"
"Plural-Forms: nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);\\n"

msgid "{count} item"
msgid_plural "{count} items"
msgstr[0] "{count} предмет"
msgstr[1] "{count} предмета"
msgstr[2] "{count} предметов"
'''
    with tempfile.TemporaryDirectory(prefix="gcn-i18n-selftest-") as raw:
        tmp = Path(raw)
        template, good, bad = tmp / "messages.pot", tmp / "good.po", tmp / "bad.po"
        template.write_text(header + template_body, encoding="utf-8")
        good.write_text(header + translation_body, encoding="utf-8")
        bad.write_text((header + translation_body).replace(
            'msgstr[2] "{count} предметов"', 'msgstr[2] "{total} предметов"'), encoding="utf-8")
        validate_placeholders(good, template)
        try:
            validate_placeholders(bad, template)
        except SystemExit:
            pass
        else:
            raise SystemExit("placeholder self-test failed to reject a broken third plural form")


def linguas():
    result = []
    for line in (PO / "LINGUAS").read_text(encoding="utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if line:
            result.extend(line.split())
    if len(result) != len(set(result)):
        raise SystemExit("po/LINGUAS contains duplicate locale identifiers")
    return result


def update():
    tools()
    require_rust_gettext()
    rust, js = source_files()
    with tempfile.TemporaryDirectory(prefix="gcn-i18n-") as raw:
        tmp = Path(raw)
        rust_pot, js_pot, merged = tmp / "rust.pot", tmp / "shell.pot", tmp / "merged.pot"
        extract(rust_pot, "Rust", RUST_KEYWORDS, rust)
        extract(js_pot, "JavaScript", JS_KEYWORDS, js)
        run("msgcat", "--use-first", "--sort-output", "--no-wrap", "--output", str(merged),
            str(rust_pot), str(js_pot))
        run("msgattrib", "--no-obsolete", "--no-wrap", "--output", str(POT), str(merged))
        content = POT.read_text(encoding="utf-8")
        content = re.sub(r'"POT-Creation-Date: [^\\]*\\n"',
                         lambda _: '"POT-Creation-Date: 1970-01-01 00:00+0000\\n"', content)
        POT.write_text(content, encoding="utf-8")
        catalog = english_catalog(js_pot, tmp)
        ids = sorted(key for key in catalog if isinstance(key, str) and key)
        SHELL_MESSAGES.write_text(json.dumps(ids, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (PO / "POTFILES.in").write_text("\n".join(rust + js) + "\n", encoding="utf-8")


def validate_registry(locales):
    registry = json.loads((PO / "languages.json").read_text(encoding="utf-8"))
    expected = ["en", *locales]
    actual = [entry.get("id") for entry in registry]
    if actual != expected or any(set(entry) != {"id", "name"} for entry in registry):
        raise SystemExit(f"po/languages.json must list exactly {expected!r} with id/name fields")
    for locale in locales:
        po_file = PO / f"{locale}.po"
        if not po_file.is_file():
            raise SystemExit(f"Missing catalog listed by LINGUAS: {po_file}")


def check_catalogs():
    tools(("msginit", "msgfmt"))
    if not POT.is_file() or not SHELL_MESSAGES.is_file() or not (PO / "POTFILES.in").is_file():
        raise SystemExit("Run scripts/translations.py update first")
    with tempfile.TemporaryDirectory(prefix="gcn-i18n-catalog-") as raw:
        catalog = english_catalog(POT, Path(raw))
    messages = json.loads(SHELL_MESSAGES.read_text(encoding="utf-8"))
    if (not isinstance(messages, list) or
            any(not isinstance(message, str) or not message for message in messages) or
            messages != sorted(set(messages)) or
            any(message not in catalog for message in messages)):
        raise SystemExit("po/shell-messages.json must contain sorted, unique POT messages")
    locales = linguas()
    validate_registry(locales)
    for locale in locales:
        validate_placeholders(PO / f"{locale}.po")
    selftest()


def check():
    tools()
    if not POT.exists() or not SHELL_MESSAGES.exists():
        raise SystemExit("Run scripts/translations.py update first")
    generated = (POT, SHELL_MESSAGES, PO / "POTFILES.in")
    saved = {path: path.read_bytes() for path in generated}
    try:
        update()
        changed = [str(path.relative_to(ROOT)) for path in generated
                   if path.read_bytes() != saved[path]]
    finally:
        for path in generated:
            path.write_bytes(saved[path])
    if changed:
        raise SystemExit("Generated translation files are stale: " + ", ".join(changed))
    check_catalogs()


def build(output):
    locales = linguas()
    validate_registry(locales)
    output.mkdir(parents=True, exist_ok=True)
    if locales:
        tools(("msgfmt",))
    for locale in locales:
        destination = output / locale / "LC_MESSAGES" / f"{DOMAIN}.mo"
        destination.parent.mkdir(parents=True, exist_ok=True)
        run("msgfmt", "--check", "--check-format", "--output-file", str(destination),
            str(PO / f"{locale}.po"))


def pseudo(output):
    tools(("msginit", "msgfmt", "msgfilter"))
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="gcn-pseudo-") as raw:
        tmp = Path(raw)
        english_po, pseudo_po = tmp / "en.po", tmp / "en_XA.po"
        run("msginit", "--no-translator", "--locale=en", "--input", str(POT),
            "--output-file", str(english_po))
        run("msgfilter", "--keep-header", "--input", str(english_po), "--output", str(pseudo_po),
            str(Path(__file__).resolve()), "_pseudo-filter")
        destination = output / "en_XA" / "LC_MESSAGES" / f"{DOMAIN}.mo"
        destination.parent.mkdir(parents=True, exist_ok=True)
        run("msgfmt", "--check", "--output-file", str(destination), str(pseudo_po))


def main():
    if sys.argv[1:] == ["_pseudo-filter"]:
        text = sys.stdin.read()
        parts = re.split(r"(\{[A-Za-z_][A-Za-z0-9_]*\}|%s)", text)
        expanded = "".join(part if PLACEHOLDER.fullmatch(part) else
                           re.sub(r"[AEIOUaeiou]", lambda match: match.group(0) * 2, part)
                           for part in parts)
        leading_newline = "\n" if expanded.startswith("\n") else ""
        trailing_newline = "\n" if expanded.endswith("\n") else ""
        if leading_newline:
            expanded = expanded[1:]
        if trailing_newline:
            expanded = expanded[:-1]
        sys.stdout.write(f"{leading_newline}[!! {expanded} !!]{trailing_newline}")
        return
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="action", required=True)
    sub.add_parser("update")
    sub.add_parser("check")
    sub.add_parser("check-catalogs")
    sub.add_parser("selftest")
    build_parser = sub.add_parser("build")
    build_parser.add_argument("--output", type=Path, default=ROOT / "target" / "locales")
    pseudo_parser = sub.add_parser("pseudo")
    pseudo_parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    actions = {"update": update, "check": check, "check-catalogs": check_catalogs,
               "selftest": selftest}
    actions.get(args.action, lambda: globals()[args.action](args.output))()


if __name__ == "__main__":
    main()
