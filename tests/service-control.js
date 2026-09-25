import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

const DRIVER='/org/example/ClipNotesTestDriver';
const DRIVER_IFACE='org.example.ClipNotesTestDriver';
const SERVICE='io.github.OleksiyM.GnomeClipNotes';

function assert(condition,message){if(!condition)throw new Error(message);}
function delay(ms){return new Promise(resolve=>GLib.timeout_add(GLib.PRIORITY_DEFAULT,ms,()=>{resolve();return GLib.SOURCE_REMOVE;}));}
function call(destination,path,iface,method,parameters=null,replyType=null){
    return new Promise((resolve,reject)=>Gio.DBus.session.call(destination,path,iface,method,parameters,replyType,
        Gio.DBusCallFlags.NONE,15000,null,(connection,result)=>{
            try{resolve(connection.call_finish(result)?.deepUnpack()??[]);}catch(error){reject(error);}
        }));
}
async function driver(method,arg=null){
    const params=arg===null?null:new GLib.Variant('(s)',[arg]);
    return call('org.gnome.Shell',DRIVER,DRIVER_IFACE,method,params);
}
async function info(){const [json]=await driver('ServiceInfo');return JSON.parse(json);}
async function waitFor(description,predicate){
    for(let i=0;i<120;i++){const value=await predicate();if(value)return value;await delay(100);}
    throw new Error(`Timed out waiting for ${description}`);
}
async function command(action){await driver('ServiceControl',action);}
async function assertBusy(action){
    const state=await info();
    assert(state.lastBusy?.action===action,`${action} should be marked busy immediately: ${JSON.stringify(state)}`);
    assert(!state.lastBusy.start&&!state.lastBusy.stop&&!state.lastBusy.restart,'all service actions should be disabled while busy');
}
async function assertIdle(running){
    const state=await waitFor('idle service menu',async()=>{const value=await info();return value.action===null?value:null;});
    assert(state.start===!running&&state.stop===running&&state.restart===running,'service action sensitivity does not match owner state');
    assert(state.status===`Service ${running?'running':'stopped'}`,`unexpected service status: ${state.status}`);
    return state;
}

try{
    let state=await assertIdle(false);
    assert(JSON.stringify(state.entries.map(item=>item.separator?'|':item.label))===JSON.stringify(['Start','Stop','Restart','|','Service stopped']),
        'service menu order or status row changed');

    await command('start');
    await assertBusy('start');
    state=await assertIdle(true);
    const firstOwner=state.owner;
    assert(firstOwner,'Start did not acquire the service bus name');
    await driver('ServiceMenuOpen','open');
    await delay(200);
    await driver('Screenshot',`${GLib.getenv('GCN_SHELL_TEST_DIR')}/service-menu-running.png`);
    await driver('ServiceMenuOpen','close');

    await command('restart');
    await assertBusy('restart');
    state=await waitFor('replacement service owner',async()=>{
        const value=await info();return value.action===null&&value.owner&&value.owner!==firstOwner?value:null;
    });
    assert(state.libraryWindows===0,'Restart unexpectedly opened Library');

    await driver('OpenNativeEditor');
    await waitFor('Native editor window',async()=>(await info()).nativeEditors>0);
    for(const action of ['stop','restart']){
        await command(action);
        await assertBusy(action);
        await waitFor(`${action} refusal with editor open`,async()=>{
            const value=await info();return value.action===null&&value.owner?value:null;
        });
        assert((await info()).owner===state.owner,'service exited while a Native editor was open');
    }
    await driver('CloseNativeEditor');
    await delay(250);
    await command('stop');
    await assertBusy('stop');
    await assertIdle(false);
    print('PASS: isolated service menu start, stop, restart and Native editor guard');
}catch(error){printerr(`FAIL: ${error.message}\n${error.stack??error}`);imports.system.exit(1);}
