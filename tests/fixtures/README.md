# Application test fixtures

`markdown-preview.md` is the production editor-process smoke fixture. It covers
heading levels, emphasis, task/nested lists, Unicode quotes, tables, links and
fenced code. It was copied unchanged from the completed WebKit prototype so
archiving that experiment does not break the main application's tests.

This is synthetic test material, not user clipboard content. The test fixture
now belongs to the application test suite; it does not depend on an archive
branch being checked out.
