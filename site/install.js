// Progressive enhancement: without JS both commands remain visible/selectable.
const tablist = document.querySelector('.install-tabs');
const tabs = [...tablist.querySelectorAll('[role="tab"]')];
function selectTab(selected) {
    for (const tab of tabs) {
        const active = tab === selected;
        tab.setAttribute('aria-selected', String(active));
        tab.tabIndex = active ? 0 : -1;
        const panel = document.getElementById(tab.getAttribute('aria-controls'));
        panel.hidden = !active;
    }
}
for (const tab of tabs) {
    const panel = document.getElementById(tab.getAttribute('aria-controls'));
    panel.setAttribute('role', 'tabpanel');
    panel.setAttribute('aria-labelledby', tab.id);
    tab.addEventListener('click', () => selectTab(tab));
    tab.addEventListener('keydown', event => {
        let next;
        if (event.key === 'ArrowRight' || event.key === 'ArrowLeft')
            next = tabs[(tabs.indexOf(tab) + 1) % tabs.length];
        else if (event.key === 'Home') next = tabs[0];
        else if (event.key === 'End') next = tabs[tabs.length - 1];
        else return;
        event.preventDefault();
        selectTab(next);
        next.focus();
    });
}
tablist.hidden = false;
selectTab(tabs[0]);
for (const button of document.querySelectorAll('[data-copy]')) {
    const label = button.querySelector('span');
    const description = button.getAttribute('aria-label');
    let resetTimer;
    button.hidden = false;
    button.addEventListener('click', async () => {
        const code = document.getElementById(button.dataset.copy);
        const status = document.getElementById(`${button.dataset.copy}-status`);
        clearTimeout(resetTimer);
        button.disabled = true;
        button.dataset.state = '';
        label.textContent = 'Copying…';
        status.textContent = '';
        try {
            await navigator.clipboard.writeText(code.textContent.trim());
            button.dataset.state = 'copied';
            label.textContent = 'Copied!';
            button.setAttribute('aria-label', `${description}: copied`);
            status.classList.add('visually-hidden');
            status.textContent = 'Command copied.';
        } catch {
            // Also works when viewing the page locally without Clipboard API access.
            const range = document.createRange();
            range.selectNodeContents(code);
            const selection = window.getSelection();
            selection.removeAllRanges();
            selection.addRange(range);
            label.textContent = 'Selected';
            button.setAttribute('aria-label', `${description}: selected for manual copying`);
            status.classList.remove('visually-hidden');
            status.textContent = 'Command selected. Press Ctrl+C (or Command+C) to copy.';
        } finally {
            button.disabled = false;
            resetTimer = setTimeout(() => {
                button.dataset.state = '';
                label.textContent = 'Copy';
                button.setAttribute('aria-label', description);
            }, 2500);
        }
    });
}
