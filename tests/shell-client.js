import Gdk from 'gi://Gdk?version=4.0';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Gtk from 'gi://Gtk?version=4.0';

const SERVICE = 'io.github.OleksiyM.GnomeClipNotes';
const SERVICE_PATH = '/io/github/OleksiyM/GnomeClipNotes';
const SERVICE_IFACE = 'io.github.OleksiyM.GnomeClipNotes.Service';
const BRIDGE_PATH = '/io/github/OleksiyM/GnomeClipNotes/Bridge';
const BRIDGE_IFACE = 'io.github.OleksiyM.GnomeClipNotes.Bridge';
const FIXTURE = 'gcn shell fixture 7f21d8';
const PASTE = 'expected active paste 93ac';
const DRIVER = '/org/example/ClipNotesTestDriver';
const DRIVER_IFACE = 'org.example.ClipNotesTestDriver';

function assert(condition, message) {
    if (!condition) throw new Error(message);
}

function delay(milliseconds) {
    return new Promise(resolve => {
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, milliseconds, () => {
            resolve();
            return GLib.SOURCE_REMOVE;
        });
    });
}

function call(destination, path, iface, method, parameters, replyType = null) {
    return new Promise((resolve, reject) => {
        Gio.DBus.session.call(destination, path, iface, method, parameters, replyType,
            Gio.DBusCallFlags.NONE, 3000, null, (connection, result) => {
                try { resolve(connection.call_finish(result)?.deepUnpack() ?? []); }
                catch (error) { reject(error); }
            });
    });
}

async function query() {
    const request = {search: FIXTURE, group_id: 0, kind: '', source: '', since: 0, until: 0, limit: 9, offset: 0};
    const [json] = await call(SERVICE, SERVICE_PATH, SERVICE_IFACE, 'Query',
        new GLib.Variant('(s)', [JSON.stringify(request)]), new GLib.VariantType('(s)'));
    return JSON.parse(json);
}

async function status(){const [json]=await call('org.gnome.Shell',BRIDGE_PATH,BRIDGE_IFACE,'GetStatus',null);return JSON.parse(json);}
async function driver(method,argument=null) {
    const parameters=argument===null?null:new GLib.Variant('(s)',[argument]);
    return call('org.gnome.Shell',DRIVER,DRIVER_IFACE,method,parameters);
}
async function transferProbe(configuration){const [json]=await driver('TransferProbe',JSON.stringify(configuration));return JSON.parse(json);}
async function overlayInfo(){const [json]=await driver('OverlayInfo');return JSON.parse(json);}
async function allItems(){const [json]=await call(SERVICE,SERVICE_PATH,SERVICE_IFACE,'Query',new GLib.Variant('(s)',['{}']));return JSON.parse(json).items;}

async function waitUntil(description, predicate, attempts = 50) {
    for (let index = 0; index < attempts; index++) {
        const value = await predicate();
        if (value) return value;
        await delay(100);
    }
    throw new Error(`Timed out waiting for ${description}`);
}

function readClipboard(clipboard) {
    return new Promise((resolve, reject) => {
        clipboard.read_text_async(null, (object, result) => {
            try { resolve(object.read_text_finish(result)); }
            catch (error) { reject(error); }
        });
    });
}

GLib.set_prgname('org.example.ClipNotesShellTest');
Gtk.init();

const window = new Gtk.Window({title: 'GnomeClipNotes Shell Test', default_width: 520, default_height: 120});
const entry = new Gtk.Entry({hexpand: true, margin_top: 24, margin_bottom: 24, margin_start: 24, margin_end: 24});
window.set_child(entry);
window.present();

try {
    await delay(500);
    entry.grab_focus();
    await delay(250);
    await call('org.gnome.Shell','/org/example/ClipNotesTestDriver','org.example.ClipNotesTestDriver','Focus',null);
    await waitUntil('test window keyboard focus',()=>Promise.resolve(window.is_active));

    const display = Gdk.Display.get_default();
    assert(display !== null, 'GTK did not connect to the isolated Wayland display');
    const clipboard = display.get_clipboard();
    const [initialStatus]=await call('org.gnome.Shell',BRIDGE_PATH,BRIDGE_IFACE,'GetStatus',null);
    print(`STATUS before copy: ${initialStatus}`);
    entry.set_text(FIXTURE);
    entry.select_region(0,-1);
    // Genuine Ctrl+C supplies the Wayland input serial needed to own clipboard.
    await call('org.gnome.Shell','/org/example/ClipNotesTestDriver','org.example.ClipNotesTestDriver','Copy',null);

    let captured;
    try { captured = await waitUntil('clipboard capture', async () => {
        const response = await query();
        return response.items?.find(item => item.content === FIXTURE) ? response : null;
    }); }catch(error){const [status]=await call('org.gnome.Shell',BRIDGE_PATH,BRIDGE_IFACE,'GetStatus',null);print(`STATUS after failed copy: ${status}`);throw error;}
    assert(captured.items.length === 1, `Expected one captured fixture, got ${captured.items.length}`);
    assert(String(captured.items[0].source ?? '').length > 0, 'Captured item has no source application');
    print('PASS capture and source attribution');
    const attempts=(await status()).capture_attempts;
    await delay(800);
    const stable = await query();
    assert(stable.items.length === 1, `Own clipboard ownership was recaptured (${stable.items.length} matches)`);
    assert(await readClipboard(clipboard) === FIXTURE, 'Clipboard text changed after capture ownership transfer');
    assert((await status()).capture_attempts===attempts,'Own clipboard changes caused another transfer');
    print('PASS ownership lifetime without duplicate capture');
    const markdownFixture='fallback markdown 4db7\n\n- first\n- **second**\n\n`exact bytes`';
    const textOffer=text=>Gdk.ContentProvider.new_union([
        Gdk.ContentProvider.new_for_bytes('text/plain;charset=UTF-8',new GLib.Bytes(new TextEncoder().encode(text))),
        Gdk.ContentProvider.new_for_bytes('text/plain;charset=utf-8',new GLib.Bytes(new TextEncoder().encode(text))),
        Gdk.ContentProvider.new_for_bytes('UTF8_STRING',new GLib.Bytes(new TextEncoder().encode(text))),
    ]);
    await transferProbe({failures:{'text/plain;charset=utf-8':1}});
    clipboard.set_content(textOffer(markdownFixture));
    await waitUntil('clipboard MIME fallback capture',async()=> (await allItems()).some(item=>item.content===markdownFixture));
    const fallbackProbe=await transferProbe({action:'stop'});
    assert(fallbackProbe.attempts[0].mime==='text/plain;charset=utf-8','Exact lower-case UTF-8 MIME was not preferred');
    assert(fallbackProbe.attempts.length===2 && fallbackProbe.attempts[1].mime==='text/plain;charset=UTF-8',`Unexpected fallback order: ${fallbackProbe.attempts.map(a=>a.mime).join(', ')}`);
    print('PASS exact MIME preference and transfer fallback preserve multiline Markdown');

    const failedFixture='all transfer formats fail 981a';
    await transferProbe({failures:{'text/plain;charset=utf-8':1,'text/plain;charset=UTF-8':1,UTF8_STRING:1}});
    clipboard.set_content(textOffer(failedFixture));
    await waitUntil('all MIME transfers attempted',async()=> (await status()).last_capture_status.startsWith('error:'));
    const failedProbe=await transferProbe({action:'stop'});
    assert(failedProbe.attempts.length===3,`Expected three failed MIME attempts, got ${failedProbe.attempts.length}`);
    assert(!(await allItems()).some(item=>item.content===failedFixture),'All-failed clipboard offer was captured');
    print('PASS all failed text formats produce no capture');

    const partialFixture='fallback after partial failure c821';
    const partialGarbage='partial transfer garbage must be discarded';
    await transferProbe({failures:{'text/plain;charset=utf-8':1},partial_text:partialGarbage});
    clipboard.set_content(textOffer(partialFixture));
    await waitUntil('fallback after partial failed transfer',async()=> (await allItems()).some(item=>item.content===partialFixture));
    const partialProbe=await transferProbe({action:'stop'});
    assert(partialProbe.attempts.length===2,'Partial failed transfer did not use one clean fallback');
    assert(!(await allItems()).some(item=>item.content.includes(partialGarbage)),'Partial failed bytes leaked into captured fallback');
    print('PASS partial failed transfer is discarded before clean fallback');

    const oversizedFixture='oversized failed transfer must not fallback e91d';
    await transferProbe({failures:{'text/plain;charset=utf-8':1},partial_size:1024*1024+1});
    clipboard.set_content(textOffer(oversizedFixture));
    await waitUntil('oversized partial transfer rejected',async()=> (await status()).last_capture_status==='ignored:too-large');
    const oversizedProbe=await transferProbe({action:'stop'});
    assert(oversizedProbe.attempts.length===1,`Oversized transfer attempted fallback: ${oversizedProbe.attempts.map(a=>a.mime).join(', ')}`);
    assert(!(await allItems()).some(item=>item.content===oversizedFixture),'Oversized failed transfer was captured');
    print('PASS oversized failed transfer cannot fallback around size policy');

    const staleFixture='cancelled fallback must not capture a13e';
    const replacementFixture='replacement owner captured 72bc';
    await transferProbe({failures:{'text/plain;charset=utf-8':16},delay_ms:250});
    clipboard.set_content(textOffer(staleFixture));
    await waitUntil('stale owner transfer start',async()=>{
        const snapshot=await transferProbe({action:'status'});
        return snapshot.attempts.length ? snapshot.attempts[0] : null;
    });
    const replacementAt=GLib.get_monotonic_time();
    clipboard.set_content(Gdk.ContentProvider.new_for_bytes('text/plain',new GLib.Bytes(new TextEncoder().encode(replacementFixture))));
    await waitUntil('replacement owner capture',async()=> (await allItems()).some(item=>item.content===replacementFixture));
    await delay(300);
    const cancellationProbe=await transferProbe({action:'stop'});
    const staleAttempts=cancellationProbe.attempts.filter(attempt=>attempt.mime!=='text/plain');
    assert(staleAttempts.length>=1 && staleAttempts.every(attempt=>attempt.mime==='text/plain;charset=utf-8'),`Cancelled stale offer retried a fallback: ${staleAttempts.map(a=>a.mime).join(', ')}`);
    assert(cancellationProbe.attempts.some(attempt=>attempt.mime==='text/plain' && attempt.time>=replacementAt),'Replacement owner was not read independently');
    assert(!(await allItems()).some(item=>item.content===staleFixture),'Cancelled owner produced an old capture');
    print('PASS owner switch cancels stale transfer without fallback');
    for(const [text,source] of [
        ['https://docs.gtk.org/gtk4/','Firefox'],
        ['A little space for what matters.','Text Editor'],
        ['cargo build --release --locked','Terminal'],
        ['Remember to take a quiet afternoon.','Calendar'],
        ['Design notes: clear type, useful space, fewer distractions.','Text Editor'],
        ['https://gnome.org','Firefox'],
        ['Release checklist: build, test, package.','Text Editor'],
        ['The next idea starts here.','Text Editor'],
    ])await call(SERVICE,SERVICE_PATH,SERVICE_IFACE,'Capture',new GLib.Variant('(sssb)',[text,source,'fixture.desktop',false]));

    await call('org.gnome.Shell', BRIDGE_PATH, BRIDGE_IFACE, 'Toggle', null);
    const opened=await waitUntil('populated modal overlay',async()=>{const info=await overlayInfo();return info.visible&&info.cards>0&&info.modal_keyboard?info:null;});
    assert(opened.cards > 0, 'Overlay opened without initial cards');
    assert(opened.modal_keyboard, 'Overlay did not acquire a keyboard modal grab');
    await driver('Key','right');
    await waitUntil('right arrow selects second card',async()=> (await overlayInfo()).selected===1);
    await driver('Key','left');
    await waitUntil('left arrow selects first card',async()=> (await overlayInfo()).selected===0);
    print('PASS arrow navigation with search focus');
    for (const layout of ['us','ru','ua']) {
        await driver('Layout',layout);
        await waitUntil('copy keyboard layout',async()=> (await overlayInfo()).layout===layout);
        await driver('Key','right');
        await waitUntil('copy second card selected',async()=> (await overlayInfo()).selected===1);
        const selected=await overlayInfo();
        const entryBefore=entry.get_text();
        await driver('Copy');
        await waitUntil('selected card copied',async()=> await readClipboard(clipboard)===selected.selected_content);
        const afterCopy=await overlayInfo();
        assert(afterCopy.visible&&afterCopy.modal_keyboard,'Copy closed overlay');
        assert(entry.get_text()===entryBefore,'Copy pasted into target');
        await driver('Key','left');
        await waitUntil('copy first card selected',async()=> (await overlayInfo()).selected===0);
        await driver('Copy');
        await waitUntil('first card copied',async()=> await readClipboard(clipboard)===opened.first_content);
        print(`PASS Ctrl+C copies selected card without paste (${layout})`);
    }
    await driver('Layout','us');
    const appearance = new Gio.Settings({schema_id:'org.gnome.desktop.interface'});
    for (const [scheme,theme] of [['prefer-dark','gcn-dark'],['default','gcn-light']]) {
        appearance.set_string('color-scheme',scheme);
        await waitUntil(`system theme ${scheme}`,async()=> (await overlayInfo()).theme===theme);
        await delay(150);
        await driver('Screenshot',`${GLib.getenv('GCN_SHELL_TEST_DIR')}/overlay-${theme}.png`);
        await driver('Click','card-context');
        await waitUntil('themed context menu',async()=> (await overlayInfo()).context_visible);
        const labels=(await overlayInfo()).context_labels;
        assert(labels.includes('View') && labels.indexOf('View') < labels.indexOf('Edit'),'View precedes Edit in card menu');
        assert((await overlayInfo()).context_shortcuts.join('|')==='Enter|F3|F4|Alt+Enter|F2|F8 / Del','Card menu displays command shortcuts');
        await delay(150);
        await driver('Screenshot',`${GLib.getenv('GCN_SHELL_TEST_DIR')}/context-${theme}.png`);
        await driver('Key','escape');
        await waitUntil('context dismissed',async()=> !(await overlayInfo()).context_visible);
    }
    print('PASS live system theme');
    for(const name of ['New Folder','New Note','Settings']){
        await driver('Hint',`hover:${name}`);
        await waitUntil(`hover hint ${name}`,async()=> (await overlayInfo()).tooltip===name);
        if(name==='Settings')await driver('Screenshot',`${GLib.getenv('GCN_SHELL_TEST_DIR')}/overlay-tooltip.png`);
        await driver('Hint','hover:');
        await waitUntil('hint dismissed on leave',async()=> !(await overlayInfo()).tooltip);
    }
    await driver('Hint','focus:New Note');
    await waitUntil('keyboard focus hint',async()=> (await overlayInfo()).tooltip==='New Note');
    await driver('CardFocus','search');
    await waitUntil('hint dismissed on focus out',async()=> !(await overlayInfo()).tooltip);
    // Closing while the delay is pending must not leave a floating label.
    await driver('Hint','hover:Settings');
    await driver('Key','escape');
    await delay(650);
    assert(!(await overlayInfo()).tooltip && !(await overlayInfo()).visible,'No tooltip survives overlay close');
    await call('org.gnome.Shell', BRIDGE_PATH, BRIDGE_IFACE, 'Toggle', null);
    await waitUntil('overlay reopens after hint check',async()=> (await overlayInfo()).visible);
    print('PASS icon hints on hover/focus and cleanup on leave/close');
    const previousDates=(await overlayInfo()).date_range;
    await driver('Click','filter-date');
    await waitUntil('date menu opens',async()=> (await overlayInfo()).context_visible);
    await driver('Click','filter-custom');
    await waitUntil('custom form stays in overlay',async()=>{
        const info=await overlayInfo();return info.visible && info.date_fields?.mapped;
    });
    await driver('Click','calendar-from');
    await waitUntil('From calendar opens',async()=> (await overlayInfo()).calendar?.visible);
    const initialCalendar=(await overlayInfo()).calendar.date;
    assert((await overlayInfo()).date_fields.from==='', 'Calendar opening does not fill empty From');
    await driver('Click','calendar-next');
    await waitUntil('Calendar changes month',async()=> (await overlayInfo()).calendar.date!==initialCalendar);
    assert((await overlayInfo()).date_fields.from==='', 'Month navigation does not fill From');
    assert(JSON.stringify((await overlayInfo()).date_range)===JSON.stringify(previousDates),'Month navigation does not filter');
    const pickedDay=(await overlayInfo()).calendar.date;
    for(const [scheme,theme] of [['prefer-dark','dark'],['default','light']]){
        appearance.set_string('color-scheme',scheme);
        await delay(180);
        await driver('Screenshot',`${GLib.getenv('GCN_SHELL_TEST_DIR')}/calendar-${theme}.png`);
    }
    await driver('Click','calendar-day');
    await waitUntil('Explicit day committed',async()=> !(await overlayInfo()).calendar && (await overlayInfo()).date_fields?.from===pickedDay);
    assert((await overlayInfo()).context_visible,'Day selection keeps From/To form open');
    await delay(250);
    await driver('Click','calendar-from');
    await waitUntil('Calendar initializes from field',async()=> (await overlayInfo()).calendar?.date===pickedDay);
    await driver('Key','escape');
    await waitUntil('Escape closes calendar only',async()=> !(await overlayInfo()).calendar);
    assert((await overlayInfo()).context_visible,'Calendar Escape retains date form');
    await delay(250);
    await driver('Click','calendar-to');
    await waitUntil('To calendar opens',async()=> (await overlayInfo()).calendar?.visible);
    await driver('Click','calendar-today');
    const today=GLib.DateTime.new_now_local().format('%Y-%m-%d');
    await waitUntil('Today fills To only',async()=> !(await overlayInfo()).calendar && (await overlayInfo()).date_fields?.to===today);
    assert((await overlayInfo()).date_fields.from===pickedDay,'Today does not overwrite the other field');
    await driver('Dates',JSON.stringify({from:'',to:''}));
    await delay(250);
    await driver('Click','calendar-from');
    await waitUntil('Today is displayed for empty field',async()=> (await overlayInfo()).calendar?.date===today);
    await driver('Click','calendar-day');
    await waitUntil('Click already selected today commits',async()=> !(await overlayInfo()).calendar && (await overlayInfo()).date_fields?.from===today);
    await delay(250);
    await driver('Click','calendar-to');
    await waitUntil('Calendar opens for outside dismissal',async()=> (await overlayInfo()).calendar?.visible);
    await driver('Click','date-from');
    await waitUntil('Outside click dismisses calendar',async()=> !(await overlayInfo()).calendar);
    assert((await overlayInfo()).context_visible && (await overlayInfo()).date_fields.to==='', 'Outside click leaves fields/form intact');
    await delay(250);
    await driver('Click','calendar-to');
    await waitUntil('Calendar opens for keyboard choice',async()=> (await overlayInfo()).calendar?.visible);
    await driver('CardFocus','calendar-day');
    await driver('Key','enter');
    await waitUntil('Keyboard day choice commits',async()=> !(await overlayInfo()).calendar && (await overlayInfo()).date_fields?.to===today);
    await delay(250);
    await driver('Click','calendar-to');
    await waitUntil('Calendar opens for adjacent month day',async()=> (await overlayInfo()).calendar?.visible);
    await driver('Click','calendar-adjacent');
    await waitUntil('Adjacent month day commits through native rebuild',async()=> !(await overlayInfo()).calendar && (await overlayInfo()).date_fields?.to!==today);
    await driver('Dates',JSON.stringify({from:'',to:''}));
    print('PASS calendar opening/month browsing are inert; day/Today commit; dismissal stays nested');
    const beforePartialDates=(await overlayInfo()).date_range;
    await driver('Dates',JSON.stringify({from:'202'}));
    assert(JSON.stringify((await overlayInfo()).date_range)===JSON.stringify(beforePartialDates),'Incomplete dates preserve filter');
    await driver('Dates',JSON.stringify({from:'',to:''}));
    await driver('Click','date-from');
    await driver('Type','2000-01-01');
    await waitUntil('From accepts keyboard typing',async()=> (await overlayInfo()).date_fields?.from==='2000-01-01');
    await driver('Key','enter');
    await waitUntil('To gains keyboard focus',async()=> (await overlayInfo()).date_fields?.focus_to);
    await driver('Type','2099-12-31');
    await waitUntil('To accepts keyboard typing',async()=> (await overlayInfo()).date_fields?.to==='2099-12-31');
    await waitUntil('valid custom range applies',async()=> (await overlayInfo()).date_range.since>0);
    const validDates=(await overlayInfo()).date_range;
    await driver('Dates',JSON.stringify({to:'1999-01-01'}));
    assert(JSON.stringify((await overlayInfo()).date_range)===JSON.stringify(validDates),'Reversed dates preserve filter');
    await driver('Dates',JSON.stringify({to:'2099-12-31'}));
    for(const [scheme,theme] of [['prefer-dark','dark'],['default','light']]){
        appearance.set_string('color-scheme',scheme);
        await delay(180);
        await driver('Screenshot',`${GLib.getenv('GCN_SHELL_TEST_DIR')}/custom-dates-${theme}.png`);
    }
    await driver('Click','date-from');
    const selectedBeforeDate=(await overlayInfo()).selected;
    await driver('Key','right');
    await driver('Key','enter');
    await waitUntil('Enter moves from From to To',async()=> (await overlayInfo()).date_fields?.focus_to);
    assert((await overlayInfo()).selected===selectedBeforeDate,'Date cursor keys do not select cards');
    await driver('Key','enter');
    await waitUntil('Enter closes completed date form',async()=> !(await overlayInfo()).context_visible);
    assert((await overlayInfo()).visible,'Date Enter does not paste/close overlay');
    await delay(250);
    await driver('Click','filter-date');
    await waitUntil('date menu reopens after Enter',async()=> (await overlayInfo()).context_labels.includes('Custom…'));
    await driver('Click','filter-custom');
    await waitUntil('custom form reopens',async()=> (await overlayInfo()).date_fields?.mapped);
    assert((await overlayInfo()).date_fields.to==='2099-12-31','Custom dates survive dismissal');
    await driver('Dates',JSON.stringify({from:'2098-01-01'}));
    await waitUntil('future dates filter results',async()=> (await overlayInfo()).cards===0);
    await driver('Key','escape');
    await delay(200);
    if((await overlayInfo()).context_visible)await driver('Key','escape');
    await waitUntil('Escape dismisses dates',async()=> !(await overlayInfo()).context_visible);
    assert((await overlayInfo()).visible,'Escape from dates keeps overlay');
    await delay(300);
    await driver('Click','filter-date');
    await waitUntil('presets reopen after Escape',async()=> (await overlayInfo()).context_labels.includes('Any time'));
    await driver('Click','filter-all');
    await waitUntil('Any time restores cards',async()=> (await overlayInfo()).cards>0 && (await overlayInfo()).date_range.since===0);
    await delay(300);
    await driver('Click','filter-date');
    await waitUntil('presets reopen after reset',async()=> (await overlayInfo()).context_labels.includes('Any time'));
    assert((await overlayInfo()).date_range.since===0,'Opening presets does not reapply custom range');
    await driver('Click','card');
    await waitUntil('outside click dismisses date menu',async()=> !(await overlayInfo()).context_visible);
    assert((await overlayInfo()).visible,'Outside dismissal does not paste underlying card');
    print('PASS custom dates live range, validation, keyboard safety, dismissal and presets');
    // Verify real Shell key routing, recording dispatch without opening app windows.
    // GTK smoke covers the corresponding real dialogs and item mutations.
    await driver('ShortcutProbe','begin');
    await driver('CardFocus','1');
    await waitUntil('keyboard focus selects card',async()=> (await overlayInfo()).selected===1);
    const shortcutItem=(await overlayInfo()).selected_id;
    let shortcutCount=0;
    for(const [key,action] of [['f3','view'],['f4','edit'],['alt+enter','info'],['f2','rename'],['f8','delete'],['delete','delete']]){
        await driver('Key',key);
        shortcutCount++;
        await waitUntil(`selected card ${key}`,async()=> (await overlayInfo()).shortcut_calls.length===shortcutCount);
        const last=(await overlayInfo()).shortcut_calls.at(-1);
        assert(last.action===action && last.id===shortcutItem,`${key} targets the selected item`);
    }
    const beforePage=(await overlayInfo());
    await driver('CardFocus',String(beforePage.cards-1));
    await driver('Key','right');
    await waitUntil('focused card crosses page',async()=> (await overlayInfo()).offset>beforePage.offset);
    await driver('Key','f3');
    shortcutCount++;
    await waitUntil('shortcut focus survives card rebuild',async()=> (await overlayInfo()).shortcut_calls.length===shortcutCount);
    assert((await overlayInfo()).shortcut_calls.at(-1).id===(await overlayInfo()).selected_id,'View targets new page selection');
    await driver('Key','left');
    await waitUntil('focused card returns to previous page',async()=> (await overlayInfo()).offset===beforePage.offset);
    await driver('CardFocus','search');
    await driver('Key','delete');
    await delay(150);
    assert((await overlayInfo()).shortcut_calls.length===shortcutCount,'Delete in search must not delete a card');
    await driver('Click','card-context');
    await waitUntil('context menu guards shortcuts',async()=> (await overlayInfo()).context_visible);
    await driver('Key','f3');
    await delay(150);
    assert((await overlayInfo()).shortcut_calls.length===shortcutCount,'Context menu owns its keyboard input');
    await driver('Key','escape');
    await driver('CardFocus','search');
    await driver('ShortcutProbe','end');
    print('PASS selected card shortcuts, focus, menu hints and search/context safety');
    await driver('ShortcutProbe','begin');
    for(const count of [0,10,11]){
        await driver('MoveFixture',String(count));
        for(const [label,action] of count===0 ? [['New Folder…','new-group-move']] : [['Folder 0','move-group-100'], ...(count>10 ? [['More Folders…','pin'],['New Folder…','new-group-move']] : [])]){
            await driver('Click','card-context');
            await waitUntil('move context opens',async()=> (await overlayInfo()).context_visible);
            const info=await overlayInfo();
            assert(info.context_labels.indexOf('Move to Notes')+1===info.context_labels.indexOf('Move to folder'),'Move to Notes precedes folder submenu');
            const expected=Array.from({length:Math.min(count,10)},(_,i)=>`Folder ${i}`);
            if(count)expected.push('');
            if(count>10)expected.push('More Folders…');
            expected.push('New Folder…');
            assert(info.move_labels.join('|')===expected.join('|'),`Move menu boundary ${count}`);
            await driver('Click','context-move');
            await waitUntil('move submenu opens',async()=> (await overlayInfo()).move_open);
            await delay(250);
            const bounds=await overlayInfo();
            assert(bounds.context_rect.y>=bounds.work_area.y && bounds.context_rect.y+bounds.context_rect.height<=bounds.work_area.y+bounds.work_area.height,'Expanded move menu stays on screen');
            await driver('MoveFocus',label);
            await delay(250);
            const calls=(await overlayInfo()).shortcut_calls.length;
            if(count===11)await driver('Screenshot',`${GLib.getenv('GCN_SHELL_TEST_DIR')}/move-submenu.png`);
            await driver('Click',`move:${label}`);
            await waitUntil(`move action ${label}`,async()=> (await overlayInfo()).shortcut_calls.length===calls+1);
            assert((await overlayInfo()).shortcut_calls.at(-1).action===action,`${label} reuses correct action`);
            await waitUntil('move menu closed',async()=> !(await overlayInfo()).context_visible);
        }
    }
    await driver('MoveFixture','restore');
    await driver('ShortcutProbe','end');
    await driver('CardFocus','search');
    print('PASS move submenu order, 0/10/11 folders and action dispatch');
    await delay(400);
    await call('org.gnome.Shell','/org/example/ClipNotesTestDriver','org.example.ClipNotesTestDriver','Screenshot',new GLib.Variant('(s)',[`${GLib.getenv('GCN_SHELL_TEST_DIR')}/overlay.png`]));
    const fullFooter=await overlayInfo();
    for (const [search,count] of [['gcn-no-matches-62930',0],['A little space for what matters.',1],['',opened.cards]]) {
        await driver('Search',search);
        await waitUntil('footer fixture query',async()=> (await overlayInfo()).cards===count);
        await delay(150); // Wait for Shell allocation after the query's render.
        const geometry=await overlayInfo();
        assert(Math.abs(geometry.paging_y-fullFooter.paging_y)<=1,'Paging moved with content height');
        assert(geometry.overlay_bottom-geometry.paging_y>200,'Paging remained in a bottom footer');
        if(count===0)await driver('Screenshot',`${GLib.getenv('GCN_SHELL_TEST_DIR')}/overlay-empty.png`);
    }
    print('PASS top paging stable with full, single-card and empty history');
    for(const width of [1408,1000]){
        await driver('Width',String(width));
        await driver('GroupFixture');
        await delay(200);
        const info=await overlayInfo();
        assert(info.search_width>=190,'Search was squeezed too far');
        assert(info.visible_groups>=2&&info.hidden_groups>0,'Folder overflow lost History, Notes or More');
        for(let i=1;i<info.toolbar.length;i++)assert(info.toolbar[i].x>=info.toolbar[i-1].x+info.toolbar[i-1].width-1,'Toolbar controls overlap');
        const last=info.toolbar.at(-1);
        assert(last.x+last.width<=16+width,'Toolbar overflows overlay');
        await driver('Screenshot',`${GLib.getenv('GCN_SHELL_TEST_DIR')}/overlay-${width}.png`);
        await driver('Click','groups-more');
        await waitUntil('folder overflow menu',async()=> (await overlayInfo()).context_visible);
        await driver('Key','escape');
        await waitUntil('folder overflow closed',async()=> !(await overlayInfo()).context_visible);
    }
    await driver('Width','1408');
    print('PASS adaptive toolbar, folder overflow and narrow layout');
    await driver('Click','page-next');
    await waitUntil('mouse next page',async()=> (await overlayInfo()).offset>0);
    await driver('Click','page-previous');
    await waitUntil('mouse previous page',async()=> (await overlayInfo()).offset===0);
    await driver('Click','filter-kind');
    await waitUntil('type filter menu',async()=> (await overlayInfo()).context_visible);
    await driver('Click','filter-link');
    await waitUntil('link filter results',async()=>{const info=await overlayInfo();return info.cards===2&&info.kinds.every(kind=>kind==='link');});
    await driver('Click','filter-kind');
    await waitUntil('type filter reopened',async()=> (await overlayInfo()).context_visible);
    await driver('Click','filter-all');
    await waitUntil('all types restored',async()=> (await overlayInfo()).cards===opened.cards);
    print('PASS dropdown type filter and reset');
    await driver('SourceFixture','begin');
    await driver('Click','filter-source');
    await delay(300);
    const sources=await overlayInfo();
    assert(sources.source_menu.entries===40,'Source menu lost entries');
    const geometry=sources.source_menu;
    await driver('Screenshot',`${GLib.getenv('GCN_SHELL_TEST_DIR')}/source-menu-open.png`);
    assert(geometry.menu.y>=geometry.area.y+8,`Source menu crosses work-area top margin: ${JSON.stringify(geometry)}`);
    assert(geometry.menu.y+geometry.menu.height<=geometry.area.y+geometry.area.height,'Source menu crosses screen bottom');
    assert(geometry.reset.y>=geometry.scroll.y+geometry.scroll.height-1,'All apps must be outside and below the scroll list');
    await driver('SourceFixture','last');
    await delay(150);
    assert((await overlayInfo()).source_menu.position>0,'Keyboard focus did not scroll to last source');
    await driver('Screenshot',`${GLib.getenv('GCN_SHELL_TEST_DIR')}/source-menu.png`);
    await driver('Click','source-last');
    await waitUntil('last source selected',async()=> (await overlayInfo()).source==='Fixture App 39');
    await driver('Click','filter-source');
    await delay(200);
    assert((await overlayInfo()).source_menu.entries===40,'Filtering narrowed the source selector');
    await driver('Click','source-reset');
    await waitUntil('All apps reset',async()=> (await overlayInfo()).source==='');
    await driver('Click','filter-source');
    await waitUntil('source menu reopened after reset',async()=> (await overlayInfo()).source_menu!==null);
    await delay(200);
    await driver('SourceFixture','last');
    await delay(150);
    await driver('Click','source-last');
    await waitUntil('source selected before removal',async()=> (await overlayInfo()).source==='Fixture App 39');
    await driver('SourceFixture','remove-selected');
    const removed=await overlayInfo();
    assert(removed.source===''&&!removed.source_options.includes('Fixture App 39'),'Deleted last source stayed selected or cached');
    await driver('Click','filter-source');
    await waitUntil('source menu before empty snapshot',async()=> (await overlayInfo()).source_menu!==null);
    await driver('SourceFixture','empty');
    await delay(200);
    assert((await overlayInfo()).source_menu.entries===0,'Open menu retained orphan sources');
    await driver('Click','source-reset');
    await waitUntil('empty source menu closed',async()=> !(await overlayInfo()).context_visible);
    await driver('SourceFixture','end');
    await waitUntil('original source data restored',async()=> (await overlayInfo()).cards===opened.cards);
    await driver('CardFocus','search');
    print('PASS bounded scrollable sources, fixed All apps, last-item mouse selection, snapshot removal and stale-filter reset');
    while((await overlayInfo()).selected>0){
        const selected=(await overlayInfo()).selected;
        await driver('Key','left');
        await waitUntil('reset selected card',async()=> (await overlayInfo()).selected===selected-1);
    }
    const page = await overlayInfo();
    for (let i=1; i<=page.cards; i++) {
        await driver('Key','right');
        await waitUntil('arrow advances selection',async()=>{
            const info=await overlayInfo();
            return i===page.cards ? info.offset===page.cards&&info.selected===0 : info.selected===i;
        });
    }
    await driver('Key','left');
    await waitUntil('arrow returns to previous page',async()=>{
        const info=await overlayInfo();
        return info.offset===0&&info.selected===page.cards-1;
    });
    for(let step=0; step<30; step++) {
        const before=await overlayInfo();
        if(!before.has_next&&before.selected===before.cards-1)break;
        await driver('Key','right');
        await waitUntil('arrow traverses history',async()=>{
            const after=await overlayInfo();
            return after.offset!==before.offset||after.selected!==before.selected;
        });
    }
    const last=await overlayInfo();
    assert(!last.has_next,'Did not reach final page');
    await driver('Key','right');
    await delay(100);
    const edge=await overlayInfo();
    assert(edge.offset===last.offset&&edge.selected===last.selected&&edge.cards>0,'Arrow left the final card');
    print('PASS arrow navigation across pages and final boundary');
    await call('org.gnome.Shell', BRIDGE_PATH, BRIDGE_IFACE, 'Toggle', null);
    const [closedJson] = await call('org.gnome.Shell', BRIDGE_PATH, BRIDGE_IFACE, 'GetStatus', null, new GLib.VariantType('(s)'));
    assert(JSON.parse(closedJson).overlay_visible === false, 'Second Bridge.Toggle did not close the overlay');
    print('PASS bridge overlay toggle');

    for (const layout of ['us','ru','ua']) {
    await driver('Layout',layout);
    await waitUntil(`layout ${layout}`,async()=> (await overlayInfo()).layout===layout);
    for (const interaction of [
        {name:'overlay Enter', text:'real enter paste b018', open:async()=>driver('Key','enter')},
        {name:'card mouse click', text:'real card click paste c129', open:async()=>driver('Click','card')},
        {name:'context-menu Paste', text:'real context paste d23a', open:async()=>{
            await driver('Click','card-context');
            await waitUntil('card context menu',async()=> (await overlayInfo()).context_visible);
            await driver('Click','context-paste');
        }},
    ]) {
        interaction.text += ` ${layout} — Русский текст, український текст`;
        await call(SERVICE,SERVICE_PATH,SERVICE_IFACE,'Capture',new GLib.Variant('(sssb)',[interaction.text,'Shell Test','org.example.ClipNotesShellTest.desktop',false]));
        entry.set_text('');
        entry.grab_focus();
        await driver('Focus');
        await waitUntil('test window keyboard focus',()=>Promise.resolve(window.is_active));
        await call('org.gnome.Shell',BRIDGE_PATH,BRIDGE_IFACE,'Toggle',null);
        await waitUntil(`${interaction.name} refreshed overlay`,async()=>{
            const info=await overlayInfo();
            return info.visible&&info.cards>0&&info.modal_keyboard&&info.first_content===interaction.text;
        });
        await interaction.open();
        try {
            await waitUntil(interaction.name,()=>Promise.resolve(entry.get_text()===interaction.text));
        } catch(error) {
            print(`INTERACTION DIAGNOSTIC ${interaction.name}: overlay=${JSON.stringify(await overlayInfo())} entry=${JSON.stringify(entry.get_text())} clipboard=${JSON.stringify(await readClipboard(clipboard))}`);
            throw error;
        }
        print(`PASS ${interaction.name} (${layout})`);
        assert((await overlayInfo()).layout===layout,'Paste changed the keyboard layout');
    }
    }
    await driver('Layout','us');

    const moveFixture='context move to notes 42cf';
    await call(SERVICE,SERVICE_PATH,SERVICE_IFACE,'Capture',new GLib.Variant('(sssb)',[moveFixture,'Shell Test','org.example.ClipNotesShellTest.desktop',false]));
    await driver('Focus');
    await call('org.gnome.Shell',BRIDGE_PATH,BRIDGE_IFACE,'Toggle',null);
    await waitUntil('move fixture in overlay',async()=> (await overlayInfo()).first_content===moveFixture);
    await driver('Click','card-context');
    await waitUntil('move context menu',async()=> (await overlayInfo()).context_visible);
    await driver('Click','context-notes');
    await waitUntil('context action updates Notes',async()=>{
        const [json]=await call(SERVICE,SERVICE_PATH,SERVICE_IFACE,'Query',new GLib.Variant('(s)',[JSON.stringify({group_id:1,search:moveFixture})]));
        return JSON.parse(json).items.some(item=>item.content===moveFixture);
    });
    assert(!(await status()).overlay_visible,'Context action left overlay open');
    print('PASS context-menu Move to Notes');

    entry.set_text('');
    entry.grab_focus();
    await delay(200);
    await call('org.gnome.Shell', BRIDGE_PATH, BRIDGE_IFACE, 'Paste', new GLib.Variant('(sb)', [PASTE, true]));
    await waitUntil('active paste into GTK entry', () => Promise.resolve(entry.get_text() === PASTE));
    assert(await readClipboard(clipboard) === PASTE, 'Bridge.Paste did not leave requested plain text on clipboard');
    print('PASS active paste and clipboard contents');
    const protectedFixture='sensitive fixture excluded';
    const providers=[
        Gdk.ContentProvider.new_for_bytes('text/plain;charset=utf-8',new GLib.Bytes(new TextEncoder().encode(protectedFixture))),
        Gdk.ContentProvider.new_for_bytes('x-kde-passwordManagerHint',new GLib.Bytes(new TextEncoder().encode('secret'))),
    ];
    await transferProbe({});
    clipboard.set_content(Gdk.ContentProvider.new_union(providers));
    await delay(400);
    const privacyProbe=await transferProbe({action:'stop'});
    assert(!(await allItems()).some(i=>i.content===protectedFixture),'Sensitive MIME content was stored');
    assert((await status()).last_capture_status==='ignored:privacy','Sensitivity filter did not reject before transfer');
    assert(privacyProbe.attempts.length===0,`Sensitivity filter read ${privacyProbe.attempts.join(', ')}`);
    print('PASS sensitive MIME excluded before reading');
    await call(SERVICE,SERVICE_PATH,SERVICE_IFACE,'Pause',new GLib.Variant('(u)',[15]));
    await delay(200);
    entry.set_text('paused fixture excluded');entry.select_region(0,-1);
    await call('org.gnome.Shell','/org/example/ClipNotesTestDriver','org.example.ClipNotesTestDriver','Copy',null);
    await delay(300);
    assert(!(await allItems()).some(i=>i.content==='paused fixture excluded'),'Paused capture stored text');
    await call(SERVICE,SERVICE_PATH,SERVICE_IFACE,'Pause',new GLib.Variant('(u)',[0]));
    await delay(200);
    entry.set_text('resumed fixture captured');entry.select_region(0,-1);
    await call('org.gnome.Shell','/org/example/ClipNotesTestDriver','org.example.ClipNotesTestDriver','Copy',null);
    await waitUntil('capture after resume',async()=> (await allItems()).some(i=>i.content==='resumed fixture captured'));
    print('PASS pause and resume');
    const [localizationJson] = await driver('Localization');
    const localization = JSON.parse(localizationJson);
    assert(localization.visible && localization.translated, 'Localized overlay still opens on first request');
    assert(localization.search === 'kept search' && localization.kind === 'text' && localization.source === 'user source', 'Localization preserves search/type/source');
    assert(localization.since === 100 && localization.until === 200 && localization.dateSelection === 'custom' && localization.dates.from === '2026-01-01', 'Localization preserves custom range');
    assert(localization.shellUnchanged && localization.englishRestored, 'Private language change leaves Shell locale alone and restores English');
    print('PASS localized overlay lifecycle, retained filters and unchanged Shell locale');
} finally {
    window.destroy();
}
