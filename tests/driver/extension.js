import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as Keyboard from 'resource:///org/gnome/shell/ui/status/keyboard.js';

const XML=`<node><interface name="org.example.ClipNotesTestDriver">
<method name="Search"><arg type="s" direction="in"/></method>
<method name="Width"><arg type="s" direction="in"/></method>
<method name="GroupFixture"/>
<method name="MoveFixture"><arg type="s" direction="in"/></method>
<method name="MoveFocus"><arg type="s" direction="in"/></method>
<method name="SourceFixture"><arg type="s" direction="in"/></method>
<method name="Layout"><arg type="s" direction="in"/></method><method name="Focus"/><method name="Copy"/><method name="Key"><arg type="s" direction="in"/></method>
<method name="ShortcutProbe"><arg type="s" direction="in"/></method>
<method name="LibraryShortcut"><arg type="s" direction="in"/></method>
<method name="CardFocus"><arg type="s" direction="in"/></method>
<method name="Hint"><arg type="s" direction="in"/></method>
<method name="Dates"><arg type="s" direction="in"/></method>
<method name="Type"><arg type="s" direction="in"/></method>
<method name="Click"><arg type="s" direction="in"/></method>
<method name="FilterClick"><arg type="s" direction="in"/></method>
<method name="OverlayInfo"><arg type="s" direction="out"/></method>
<method name="Localization"><arg type="s" direction="out"/></method>
<method name="Screenshot"><arg type="s" direction="in"/></method>
<method name="TransferProbe"><arg type="s" direction="in"/><arg type="s" direction="out"/></method>
<method name="ServiceControl"><arg type="s" direction="in"/></method>
<method name="ServiceInfo"><arg type="s" direction="out"/></method>
<method name="ServiceMenuOpen"><arg type="s" direction="in"/></method>
<method name="OpenNativeEditor"/><method name="CloseNativeEditor"/>
</interface></node>`;
export default class TestDriver extends Extension {
    enable() {
        const seat=Clutter.get_default_backend().get_default_seat();
        this._seat=seat;
        this._events=[];
        this._eventId=global.stage.connect('captured-event',(_actor,event)=>{
            if([Clutter.EventType.MOTION,Clutter.EventType.BUTTON_PRESS,Clutter.EventType.BUTTON_RELEASE].includes(event.type())){
                const actor=global.stage.get_event_actor(event);
                this._events.push({type:event.type(),coords:event.get_coords(),style:actor?.style_class??'',name:actor?.name??''});
                this._events=this._events.slice(-8);
            }
            return Clutter.EVENT_PROPAGATE;
        });
        this._keyboard=seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        this._pointer=seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        this._object=Gio.DBusExportedObject.wrapJSObject(XML,{
            FilterClickAsync:([json],invocation)=>{
                const window=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='ClipNotes filters test');
                if(!window){invocation.return_dbus_error('org.example.Error','Missing filter test window');return;}
                const point=JSON.parse(json);
                // GTK widget coordinates exclude the CSD shadow in the buffer.
                const bounds=window.get_frame_rect();
                if(!Number.isFinite(point.x)||!Number.isFinite(point.y)){invocation.return_dbus_error('org.example.Error','Invalid coordinates');return;}
                const click=()=>{
                this._pointer.notify_absolute_motion(GLib.get_monotonic_time(),Math.round(bounds.x+point.x),Math.round(bounds.y+point.y));
                GLib.timeout_add(GLib.PRIORITY_DEFAULT,60,()=>{
                    this._pointer.notify_button(GLib.get_monotonic_time(),Clutter.BUTTON_PRIMARY,Clutter.ButtonState.PRESSED);
                    GLib.timeout_add(GLib.PRIORITY_DEFAULT,30,()=>{
                        this._pointer.notify_button(GLib.get_monotonic_time(),Clutter.BUTTON_PRIMARY,Clutter.ButtonState.RELEASED);
                        invocation.return_value(null);
                        return GLib.SOURCE_REMOVE;
                    });
                    return GLib.SOURCE_REMOVE;
                });
                return GLib.SOURCE_REMOVE;
                };
                if(Main.overview.visible){
                    Main.overview.hide();
                    window.activate(global.get_current_time());
                    this._seat.warp_pointer(300,400);
                    GLib.timeout_add(GLib.PRIORITY_DEFAULT,400,click);
                }else click();
            },
            Search:text=>this._overlay()._search.set_text(text),
            WidthAsync:async([value],invocation)=>{
                try {
                    const overlay=this._overlay();
                    overlay._actor.width=Number(value);
                    overlay._pageSize=Math.min(9,Math.max(1,Math.floor((Number(value)-36)/208)));
                    overlay._query.offset=0;
                    await overlay.refresh();
                    invocation.return_value(null);
                } catch(error) { invocation.return_dbus_error('org.example.Error',error.message); }
            },
            GroupFixture:()=>{
                const overlay=this._overlay();
                overlay._renderGroups(Array.from({length:9},(_,i)=>({id:i+2,name:i===2?'LongNameFolder3 with extra words':`Folder${i+1}`})));
                overlay._renderCards();
            },
            MoveFixture:count=>{
                const overlay=this._overlay();
                if(count==='restore'){
                    overlay._moveGroups=this._savedMoveGroups;
                    this._savedMoveGroups=null;
                } else {
                    this._savedMoveGroups??=overlay._moveGroups;
                    overlay._moveGroups=Array.from({length:Number(count)},(_,i)=>({id:100-i,name:`Folder ${i}`}));
                }
            },
            MoveFocus:label=>this._overlay()._context._getMenuItems().find(item=>item.label?.text==='Move to folder').menu._getMenuItems().find(item=>item.label?.text===label).grab_key_focus(),
            SourceFixtureAsync:async([mode],invocation)=>{
                try {
                    const overlay=this._overlay();
                    if(mode==='begin'){
                        this._originalQuery=overlay._service.QueryAsync;
                        this._fixtureSources=Array.from({length:40},(_,i)=>`Fixture App ${String(i).padStart(2,'0')}`);
                        overlay._service.QueryAsync=async(...args)=>{
                            const [json]=await this._originalQuery.apply(overlay._service,args);
                            return [JSON.stringify({...JSON.parse(json),sources:this._fixtureSources})];
                        };
                    } else if(mode==='remove-selected'){
                        this._fixtureSources=this._fixtureSources.filter(name=>name!==overlay._query.source);
                    } else if(mode==='empty'){
                        this._fixtureSources=[];
                    } else if(mode==='last'){
                        overlay._context._sourceSection._getMenuItems().at(-1).grab_key_focus();
                    } else if(mode==='end'){
                        overlay._service.QueryAsync=this._originalQuery;
                        this._originalQuery=null;
                    } else throw new Error(`Unknown source fixture: ${mode}`);
                    if(mode!=='last')await overlay.refresh();
                    invocation.return_value(null);
                }catch(error){invocation.return_dbus_error('org.example.Error',error.message);}
            },
            Layout:id=>{
                const manager=Keyboard.getInputSourceManager();
                const source=Object.values(manager.inputSources).find(source=>source.id===id);
                if(!source)throw new Error(`Missing test layout: ${id}`);
                source.activate(true);
            },
            Focus:()=>{
                Main.overview.hide();
                const window=global.get_window_actors().map(a=>a.meta_window).find(w=>w.get_title()==='GnomeClipNotes Shell Test');
                if(!window)throw new Error('Test window not mapped');
                window.activate(global.get_current_time());
                // Initialize the headless seat away from the top-edge barrier,
                // outside the future overlay, as with normal desktop use.
                this._seat.warp_pointer(300,400);
            },
            Copy:()=>{
                for(const [key,state] of [[29,Clutter.KeyState.PRESSED],[46,Clutter.KeyState.PRESSED],[46,Clutter.KeyState.RELEASED],[29,Clutter.KeyState.RELEASED]])
                    this._keyboard.notify_key(GLib.get_monotonic_time(),key,state);
            },
            Key:key=>{
                const alt=key.startsWith('alt+');
                if(alt)key=key.slice(4);
                const keyval={enter:Clutter.KEY_Return,escape:Clutter.KEY_Escape,left:Clutter.KEY_Left,right:Clutter.KEY_Right,f2:Clutter.KEY_F2,f3:Clutter.KEY_F3,f4:Clutter.KEY_F4,f8:Clutter.KEY_F8,delete:Clutter.KEY_Delete}[key];
                if(!keyval)throw new Error(`Unknown key target: ${key}`);
                if(alt)this._keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Alt_L,Clutter.KeyState.PRESSED);
                this._keyboard.notify_keyval(GLib.get_monotonic_time(),keyval,Clutter.KeyState.PRESSED);
                this._keyboard.notify_keyval(GLib.get_monotonic_time(),keyval,Clutter.KeyState.RELEASED);
                if(alt)this._keyboard.notify_keyval(GLib.get_monotonic_time(),Clutter.KEY_Alt_L,Clutter.KeyState.RELEASED);
            },
            ShortcutProbe:mode=>{
                const overlay=this._overlay();
                if(mode==='begin'){
                    this._shortcutCalls=[];
                    this._originalActivate=overlay._activateApp;
                    overlay._activateApp=(action,id)=>this._shortcutCalls.push({action,id});
                }else if(this._originalActivate){
                    overlay._activateApp=this._originalActivate;
                    this._originalActivate=null;
                }
            },
            LibraryShortcut:mode=>{
                const extension=Main.extensionManager.lookup('gnome-clip-notes@oleksiym.github.io').stateObj;
                if(mode==='begin'){
                    this._libraryCalls=[];
                    this._libraryActivate=extension.activate;
                    this._libraryBinding=extension._settings.get_strv('library-shortcut');
                    extension.activate=(action,id)=>this._libraryCalls.push({action,id});
                }else if(mode==='end'){
                    extension.activate=this._libraryActivate;
                    extension._settings.set_strv('library-shortcut',this._libraryBinding);
                }else if(mode==='alternate'||mode==='disabled'){
                    extension._settings.set_strv('library-shortcut',mode==='alternate'?['<Control><Alt>b']:[]);
                }else{
                    const keys=mode==='super'?[125,48]:[29,56,48];
                    for(const code of keys)this._keyboard.notify_key(GLib.get_monotonic_time(),code,Clutter.KeyState.PRESSED);
                    for(const code of keys.reverse())this._keyboard.notify_key(GLib.get_monotonic_time(),code,Clutter.KeyState.RELEASED);
                }
            },
            CardFocus:index=>{
                const overlay=this._overlay();
                if(index==='calendar-day'){
                    const calendar=overlay._datePicker._calendar;
                    calendar._buttons.find(b=>b._date.toDateString()===calendar._selectedDate.toDateString()).grab_key_focus();
                }
                else if(index==='search')overlay._search.get_clutter_text().grab_key_focus();
                else overlay._cards.get_children()[Number(index)].grab_key_focus();
            },
            Hint:target=>{
                const overlay=this._overlay();
                const [mode,name]=target.split(':');
                const find=actor=>actor.accessible_name===name ? actor : actor.get_children().map(find).find(Boolean);
                const actor=name ? find(overlay._actor) : overlay._search;
                if(!actor)throw new Error(`Missing hint target: ${name}`);
                if(mode==='focus')actor.grab_key_focus();
                else {
                    const [x,y]=actor.get_transformed_position();
                    const [width,height]=actor.get_transformed_size();
                    this._seat.warp_pointer(x+width/2,y+height/2);
                    this._pointer.notify_relative_motion(GLib.get_monotonic_time(),1,1);
                    this._pointer.notify_relative_motion(GLib.get_monotonic_time(),-1,-1);
                }
            },
            Dates:json=>{
                const fields=this._overlay()._context?._dateFields;
                if(!fields)throw new Error('Missing custom date form');
                const values=JSON.parse(json);
                if(values.from!==undefined)fields.from.set_text(values.from);
                if(values.to!==undefined)fields.to.set_text(values.to);
            },
            Type:text=>{
                for(const character of text){
                    const key=character.codePointAt(0);
                    if(key<32 || key>126)throw new Error('ASCII test input required');
                    this._keyboard.notify_keyval(GLib.get_monotonic_time(),key,Clutter.KeyState.PRESSED);
                    this._keyboard.notify_keyval(GLib.get_monotonic_time(),key,Clutter.KeyState.RELEASED);
                }
            },
            ClickAsync:([target],invocation)=>{
                const overlay=this._overlay();
                let actor;
                let button=Clutter.BUTTON_PRIMARY;
                if(target==='groups-more')actor=overlay?._moreGroups;
                else if(target==='page-next')actor=overlay?._nextButton;
                else if(target==='page-previous')actor=overlay?._previousButton;
                else if(target==='card')actor=overlay?._cards?.get_children()[0];
                else if(target==='card-context'){actor=overlay?._cards?.get_children()[0];button=Clutter.BUTTON_SECONDARY;}
                else if(target==='context-paste')actor=overlay?._context?.box?.get_children()[0];
                else if(target==='context-notes')actor=overlay?._context?._getMenuItems().find(item=>item.label?.text==='Move to Notes');
                else if(target==='context-move')actor=overlay?._context?._getMenuItems().find(item=>item.label?.text==='Move to folder');
                else if(target.startsWith('move:'))actor=overlay?._context?._getMenuItems().find(item=>item.label?.text==='Move to folder')?.menu._getMenuItems().find(item=>item.label?.text===target.slice(5));
                else if(target==='filter-kind')actor=overlay?._kindButton;
                else if(target==='filter-source')actor=overlay?._sourceButton;
                else if(target==='source-reset')actor=overlay?._context?._sourceReset;
                else if(target==='source-last')actor=overlay?._context?._sourceSection?._getMenuItems().at(-1);
                else if(target==='filter-date')actor=overlay?._dateButton;
                else if(target==='filter-custom')actor=overlay?._context?._getMenuItems().find(item=>item.label?.text==='Custom…');
                else if(target==='date-from')actor=overlay?._context?._dateFields?.from;
                else if(target==='calendar-from')actor=overlay?._context?._dateFields?.from._calendarButton;
                else if(target==='calendar-to')actor=overlay?._context?._dateFields?.to._calendarButton;
                else if(target==='calendar-next')actor=overlay?._datePicker?._calendar._forwardButton;
                else if(target==='calendar-day'){
                    const calendar=overlay?._datePicker?._calendar;
                    actor=calendar?._buttons.find(b=>b._date.toDateString()===calendar._selectedDate.toDateString());
                }
                else if(target==='calendar-adjacent'){
                    const calendar=overlay?._datePicker?._calendar;
                    actor=calendar?._buttons.find(b=>b._date.getMonth()!==calendar._selectedDate.getMonth());
                }
                else if(target==='calendar-today')actor=overlay?._datePicker?._getMenuItems().find(item=>item.label?.text==='Today');
                else if(target==='filter-link')actor=overlay?._context?.box?.get_children()[2];
                else if(target==='filter-all')actor=overlay?._context?.box?.get_children()[0];
                else throw new Error(`Unknown click target: ${target}`);
                if(!actor?.visible){invocation.return_dbus_error('org.example.Error',`Click target is not visible: ${target}`);return;}
                GLib.timeout_add(GLib.PRIORITY_DEFAULT,50,()=>{
                    const [x,y]=actor.get_transformed_position();
                    const [width,height]=actor.get_transformed_size();
                    const pointerX=x+width/2;
                    const pointerY=y+height/2;
                    this._lastClick={target,x,y,width,height,pointerX,pointerY};
                    this._seat.warp_pointer(pointerX,pointerY);
                    this._pointer.notify_relative_motion(GLib.get_monotonic_time(),1,1);
                    this._pointer.notify_relative_motion(GLib.get_monotonic_time(),-1,-1);
                    GLib.timeout_add(GLib.PRIORITY_DEFAULT,50,()=>{
                        const picked=global.stage.get_actor_at_pos(Clutter.PickMode.REACTIVE,pointerX,pointerY);
                        this._lastClick.picked_style=picked?.style_class??'';
                        this._lastClick.pointer=global.get_pointer().slice(0,2);
                        this._pointer.notify_button(GLib.get_monotonic_time(),button,Clutter.ButtonState.PRESSED);
                        GLib.timeout_add(GLib.PRIORITY_DEFAULT,30,()=>{
                            this._pointer.notify_button(GLib.get_monotonic_time(),button,Clutter.ButtonState.RELEASED);
                            invocation.return_value(null);
                            return GLib.SOURCE_REMOVE;
                        });
                        return GLib.SOURCE_REMOVE;
                    });
                    return GLib.SOURCE_REMOVE;
                });
            },
            LocalizationAsync: async (_args, invocation) => {
                const extension = Main.extensionManager.lookup('gnome-clip-notes@oleksiym.github.io').stateObj;
                const service = extension._service;
                const original = service.QueryAsync;
                const environment = [GLib.getenv('LANGUAGE'), GLib.getenv('LC_ALL'), GLib.getenv('LANG')];
                const previous = {...extension._overlay._query};
                const pause = () => new Promise(resolve => GLib.timeout_add(GLib.PRIORITY_DEFAULT, 250, () => {resolve(); return GLib.SOURCE_REMOVE;}));
                try {
                    const [json] = await original.call(service, JSON.stringify({metadata_only: true}));
                    const result = JSON.parse(json);
                    result.localization.language = 'test';
                    for (const key of Object.keys(result.localization.messages))
                        result.localization.messages[key] = `[!! ${key} — long translation !!]`;
                    service.QueryAsync = async query => {
                        if (JSON.parse(query).metadata_only) return [JSON.stringify(result)];
                        const [response] = await original.call(service, query);
                        const snapshot = JSON.parse(response);
                        // This fixture tests preserving an existing source across
                        // catalog replacement, not retaining a deleted source.
                        snapshot.sources = [...snapshot.sources, 'user source'];
                        return [JSON.stringify(snapshot)];
                    };
                    extension._overlay.hide();
                    extension._overlay._search.set_text('kept search');
                    Object.assign(extension._overlay._query, {since: 100, until: 200, kind: 'text', source: 'user source'});
                    extension._overlay._customDates = {from: '2026-01-01', to: '2026-01-02'};
                    extension._overlay._dateButton._selectedValue = 'custom';
                    // Exercises the initial-opening path: metadata arrives while
                    // show() is waiting for ensureService(), without losing it.
                    await extension._overlay.show();
                    await pause();
                    const overlay = extension._overlay;
                    const output = {
                        visible: overlay.visible,
                        translated: overlay._search.hint_text === result.localization.messages['Search clipboard and notes'],
                        search: overlay._query.search,
                        kind: overlay._query.kind, source: overlay._query.source,
                        since: overlay._query.since, until: overlay._query.until,
                        dates: overlay._customDates,
                        dateSelection: overlay._dateButton._selectedValue,
                        shellUnchanged: JSON.stringify(environment) === JSON.stringify([GLib.getenv('LANGUAGE'), GLib.getenv('LC_ALL'), GLib.getenv('LANG')]),
                    };
                    service.QueryAsync = original;
                    await extension._reloadServiceSettings();
                    await pause();
                    output.englishRestored = extension._overlay._search.hint_text === 'Search clipboard and notes';
                    invocation.return_value(new GLib.Variant('(s)', [JSON.stringify(output)]));
                } catch (error) {
                    invocation.return_dbus_error('org.example.Error', error.message);
                } finally {
                    service.QueryAsync = original;
                    extension._overlay.hide();
                    extension._overlay._search.set_text(previous.search);
                    extension._overlay._query = previous;
                    await extension._reloadServiceSettings();
                }
            },
            OverlayInfo:()=>{
                const overlay=this._overlay();
                const paging=overlay?._pageLabel?.get_parent();
                const rect=a=>{const [x,y]=a.get_transformed_position();return {x,y,width:a.width,height:a.height};};
                const area=Main.layoutManager.getWorkAreaForMonitor(Main.layoutManager.primaryIndex);
                return JSON.stringify({
                    calendar:overlay?._datePicker ? {
                        visible:overlay._datePicker.isOpen,
                        date:[overlay._datePicker._calendar._selectedDate.getFullYear(),
                            String(overlay._datePicker._calendar._selectedDate.getMonth()+1).padStart(2,'0'),
                            String(overlay._datePicker._calendar._selectedDate.getDate()).padStart(2,'0')].join('-'),
                    } : null,
                    date_range:{since:overlay?._query.since,until:overlay?._query.until},
                    date_fields:overlay?._context?._dateFields ? {
                        from:overlay._context._dateFields.from.get_text(),
                        to:overlay._context._dateFields.to.get_text(),
                        mapped:overlay._context._dateFields.from.mapped,
                        hint:overlay._context._dateFields.hint.text,
                        focus_to:global.stage.get_key_focus()===overlay._context._dateFields.to.clutter_text,
                    } : null,
                    tooltip:overlay?._tooltip?.text??'',
                    paging_y:paging?.get_transformed_position()[1]??0,
                    toolbar:overlay?._toolbar?.get_children().filter(a=>a.visible).map(rect)??[],
                    search_width:overlay?._search?.width??0,
                    groups_second_row:overlay?._groupsSecondRow??false,
                    visible_groups:overlay?._visibleGroupCount??0,
                    hidden_groups:overlay?._hiddenGroupCount??0,
                    overlay_width:overlay?._actor.width??0,
                    overlay_bottom:overlay?(overlay._actor.get_transformed_position()[1]+overlay._actor.height):0,
                    selected:overlay?._selected??0,
                    selected_id:overlay?._items?.[overlay?._selected]?.id??null,
                    card_has_focus:!!(overlay && global.stage.get_key_focus() && overlay._cards.contains(global.stage.get_key_focus())),
                    shortcut_calls:this._shortcutCalls??[],
                    library_calls:this._libraryCalls??[],
                    selected_content:overlay?._items?.[overlay?._selected]?.content??'',
                    offset:overlay?._query.offset??0,
                    has_next:overlay?._hasNext??false,
                    theme:overlay?._themeClass,
                    layout:Keyboard.getInputSourceManager().currentSource?.id,
                    visible:overlay?.visible??false,
                    cards:overlay?._items?.length??0,
                    first_content:overlay?._items?.[0]?.content??'',
                    kinds:overlay?._items?.map(item=>item.kind)??[],
                    context_visible:overlay?._context?.isOpen??false,
                    source:overlay?._query.source??'',
                    source_options:overlay?._sourceOptions?.map(option=>option[1])??[],
                    source_menu:overlay?._context?._sourceSection ? {
                        menu:rect(overlay._context.actor),
                        reset:rect(overlay._context._sourceReset),
                        scroll:rect(overlay._context._sourceScroll),
                        entries:overlay._context._sourceSection.numMenuItems,
                        position:(overlay._context._sourceScroll.vadjustment??overlay._context._sourceScroll.vscroll.adjustment).value,
                        area:{x:area.x,y:area.y,width:area.width,height:area.height},
                    } : null,
                    context_labels:overlay?._context?._getMenuItems().map(item=>item.label?.text??'')??[],
                    move_labels:overlay?._context?._getMenuItems().find(item=>item.label?.text==='Move to folder')?.menu._getMenuItems().map(item=>item.label?.text??'')??[],
                    move_open:overlay?._context?._getMenuItems().find(item=>item.label?.text==='Move to folder')?.menu.isOpen??false,
                    context_rect:overlay?._context ? rect(overlay._context.actor) : null,
                    work_area:{x:area.x,y:area.y,width:area.width,height:area.height},
                    context_shortcuts:overlay?._context?._getMenuItems().flatMap(item=>(item.get_children?.()??[]).filter(child=>child.has_style_class_name('gcn-menu-shortcut')).map(child=>child.text))??[],
                    modal_keyboard:this._hasModalGrab(overlay?._grab),
                    error:overlay?._lastError??null,
                    last_click:this._lastClick??null,
                    events:this._events,
                });
            },
            ScreenshotAsync:([path],invocation)=>{
                if(!path.startsWith(`${GLib.getenv('GCN_SHELL_TEST_DIR')}/`)){invocation.return_dbus_error('org.example.Error','Invalid test path');return;}
                const stream=Gio.File.new_for_path(path).replace(null,false,Gio.FileCreateFlags.PRIVATE,null);
                const screenshot=new Shell.Screenshot();
                screenshot.screenshot(false,stream,(object,result)=>{
                    try{object.screenshot_finish(result);stream.close(null);invocation.return_value(null);}catch(error){invocation.return_dbus_error('org.example.Error',error.message);}
                });
            },
            TransferProbe:json=>{
                const command=JSON.parse(json);
                if(command.action==='stop'){
                    const attempts=this._transferProbe?.attempts??[];
                    this._restoreTransferProbe();
                    return JSON.stringify({attempts});
                }
                if(command.action==='status')
                    return JSON.stringify({attempts:this._transferProbe?.attempts??[]});
                this._restoreTransferProbe();
                const extension=Main.extensionManager.lookup('gnome-clip-notes@oleksiym.github.io')?.stateObj;
                const selection=extension?._selection;
                if(!selection)throw new Error('Missing clipboard selection');
                const originalAsync=selection.transfer_async;
                const originalFinish=selection.transfer_finish;
                const failures=new Map(Object.entries(command.failures??{}).map(([mime,count])=>[mime,Number(count)]));
                const fakeResults=new Set();
                const attempts=[];
                const timeoutIds=new Set();
                selection.transfer_async=(type,mime,maxBytes,output,cancellable,callback)=>{
                    attempts.push({mime:String(mime),generation:extension._captureGeneration,time:GLib.get_monotonic_time()});
                    const remaining=failures.get(String(mime))??0;
                    if(remaining<=0)return originalAsync.call(selection,type,mime,maxBytes,output,cancellable,callback);
                    failures.set(String(mime),remaining-1);
                    if(command.partial_text)
                        output.write_all(new TextEncoder().encode(command.partial_text),null);
                    if(command.partial_size)
                        output.write_all(new Uint8Array(Number(command.partial_size)).fill(120),null);
                    const result={mime:String(mime)};
                    fakeResults.add(result);
                    const timeoutId=GLib.timeout_add(GLib.PRIORITY_DEFAULT,Number(command.delay_ms??0),()=>{
                        timeoutIds.delete(timeoutId);
                        callback(selection,result);
                        return GLib.SOURCE_REMOVE;
                    });
                    timeoutIds.add(timeoutId);
                };
                selection.transfer_finish=result=>{
                    if(fakeResults.has(result)){
                        fakeResults.delete(result);
                        throw new Error(`Injected transfer failure for ${result.mime}`);
                    }
                    return originalFinish.call(selection,result);
                };
                this._transferProbe={selection,originalAsync,originalFinish,attempts,timeoutIds};
                return JSON.stringify({attempts:[]});
            },
            ServiceControlAsync:([action],invocation)=>{
                const extension=Main.extensionManager.lookup('gnome-clip-notes@oleksiym.github.io')?.stateObj;
                if(!extension){invocation.return_dbus_error('org.example.Error','Missing ClipNotes extension');return;}
                extension.controlService(action);
                const indicator=extension._indicator;
                this._lastServiceBusy={action:extension._serviceAction,
                    start:indicator?._startService?.sensitive??false,
                    stop:indicator?._stopService?.sensitive??false,
                    restart:indicator?._restartService?.sensitive??false};
                // A duplicate request in the same main-loop turn must be ignored.
                extension.controlService(action);
                invocation.return_value(null);
            },
            ServiceInfo:()=>{
                const extension=Main.extensionManager.lookup('gnome-clip-notes@oleksiym.github.io')?.stateObj;
                const menu=extension?._indicator?._serviceMenu?.menu;
                return JSON.stringify({
                    action:extension?._serviceAction??null,
                    locked:extension?._isLocked(),
                    ensuring:Boolean(extension?._ensurePromise),
                    owner:extension?._service?.g_name_owner??null,
                    entries:menu?._getMenuItems().map(item=>({
                        label:item.label?.text??null,
                        separator:item.constructor.name==='PopupSeparatorMenuItem',
                        sensitive:item.sensitive??null,
                    }))??[],
                    start:extension?._indicator?._startService?.sensitive??false,
                    stop:extension?._indicator?._stopService?.sensitive??false,
                    restart:extension?._indicator?._restartService?.sensitive??false,
                    status:extension?._indicator?._serviceStatus?.label?.text??'',
                    libraryWindows:global.get_window_actors().map(actor=>actor.meta_window)
                        .filter(window=>window.get_gtk_application_id()==='io.github.OleksiyM.GnomeClipNotes').length,
                    nativeEditors:global.get_window_actors().map(actor=>actor.meta_window)
                        .filter(window=>window.get_title().includes('New Note')).length,
                    lastBusy:this._lastServiceBusy??null,
                });
            },
            ServiceMenuOpen:mode=>{
                const indicator=Main.extensionManager.lookup('gnome-clip-notes@oleksiym.github.io')?.stateObj?._indicator;
                if(mode==='open'){
                    indicator?.menu.open();
                    indicator?._serviceMenu?.menu.open();
                }else{
                    indicator?._serviceMenu?.menu.close();
                    indicator?.menu.close();
                }
            },
            OpenNativeEditor:()=>{
                const extension=Main.extensionManager.lookup('gnome-clip-notes@oleksiym.github.io')?.stateObj;
                extension?._service?.ActivateRemote('new-note',0);
            },
            CloseNativeEditor:()=>{
                const owner=Main.extensionManager.lookup('gnome-clip-notes@oleksiym.github.io')?.stateObj?._service?.g_name_owner;
                if(!owner)throw new Error('Service has no bus owner');
                const [pid]=Gio.DBus.session.call_sync('org.freedesktop.DBus','/org/freedesktop/DBus',
                    'org.freedesktop.DBus','GetConnectionUnixProcessID',new GLib.Variant('(s)',[owner]),
                    new GLib.VariantType('(u)'),Gio.DBusCallFlags.NONE,3000,null).deepUnpack();
                const window=global.get_window_actors().map(actor=>actor.meta_window)
                    .find(candidate=>candidate.get_pid()===pid&&candidate.get_title().includes('New Note'));
                if(!window)throw new Error('Missing owned Native editor window');
                window.delete(global.get_current_time());
            },
        });
        this._object.export(Gio.DBus.session,'/org/example/ClipNotesTestDriver');
    }
    _hasModalGrab(grab){
        if(!grab)return false;
        if(typeof grab.is_revoked==='function')return !grab.is_revoked();
        if(typeof grab.get_seat_state==='function'&&Clutter.GrabState)
            return Boolean(grab.get_seat_state()&Clutter.GrabState.KEYBOARD);
        return true;
    }
    _overlay(){return Main.extensionManager.lookup('gnome-clip-notes@oleksiym.github.io')?.stateObj?._overlay;}
    _restoreTransferProbe(){
        if(!this._transferProbe)return;
        for(const id of this._transferProbe.timeoutIds)GLib.source_remove(id);
        this._transferProbe.selection.transfer_async=this._transferProbe.originalAsync;
        this._transferProbe.selection.transfer_finish=this._transferProbe.originalFinish;
        this._transferProbe=null;
    }
    disable(){this._restoreTransferProbe();if(this._eventId)global.stage.disconnect(this._eventId);this._object?.unexport();this._object=null;this._keyboard=null;this._pointer=null;this._seat=null;}
}
