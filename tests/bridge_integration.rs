use rusqlite::Connection;
use serde_json::Value;
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    os::fd::AsRawFd,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(1);

struct TempProfile {
    root: PathBuf,
    data_home: PathBuf,
    config_home: PathBuf,
}

impl TempProfile {
    fn new(label: &str) -> Self {
        let seq = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "gcn-bridge-test-{}-{}-{seq}",
            std::process::id(),
            label
        ));
        let data_home = root.join("data");
        let config_home = root.join("config");
        fs::create_dir_all(&data_home).unwrap();
        fs::create_dir_all(&config_home).unwrap();
        Self {
            root,
            data_home,
            config_home,
        }
    }
}

impl Drop for TempProfile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn spawn_bridge(profile: &TempProfile) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_gnome-clip-notes"))
        .arg("--bridge")
        .env("XDG_DATA_HOME", &profile.data_home)
        .env("XDG_CONFIG_HOME", &profile.config_home)
        .env("DBUS_SESSION_BUS_ADDRESS", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gnome-clip-notes --bridge")
}

fn send_recv(stdin: &mut impl Write, stdout: &mut impl BufRead, req: &str) -> Value {
    writeln!(stdin, "{req}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    let n = stdout.read_line(&mut line).unwrap();
    assert!(n > 0, "unexpected EOF on bridge stdout");
    serde_json::from_str(&line).expect("bridge stdout line must be valid JSON")
}

#[test]
fn bridge_process_lifecycle_jsonl_and_no_update_lock() {
    let profile = TempProfile::new("lifecycle");
    let mut child = spawn_bridge(&profile);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let ping = send_recv(
        &mut stdin,
        &mut stdout,
        r#"{"request_id":"req-1","method":"ping"}"#,
    );
    assert_eq!(ping["type"], "response");
    assert_eq!(ping["request_id"], "req-1");
    assert_eq!(ping["ok"], true);
    assert_eq!(ping["result"]["max_content"], 1_048_576);

    let folders = send_recv(
        &mut stdin,
        &mut stdout,
        r#"{"request_id":2,"method":"list_folders"}"#,
    );
    assert_eq!(folders["ok"], true);
    assert_eq!(folders["result"]["folders"][0]["id"], 1);
    assert_eq!(folders["result"]["folders"][0]["name"], "Notes");

    // Verify update.lock is NOT held while --bridge is idle between requests.
    let lock_path = profile.data_home.join("gnome-clip-notes/update.lock");
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .unwrap();
    let rc = unsafe { libc::flock(lock_file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    assert_eq!(
        rc,
        0,
        "bridge must not hold update.lock while idle (errno={})",
        std::io::Error::last_os_error()
    );
    drop(lock_file);

    // Seed a History item and a Notes item in the isolated database.
    let db_path = profile.data_home.join("gnome-clip-notes/data.db");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO items(title,content,kind,origin,created_at,updated_at,copied_at,group_id) VALUES('','Hist','text','clipboard',100,100,100,0)",
        [],
    )
    .unwrap();
    let history_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO items(title,content,kind,origin,created_at,updated_at,copied_at,group_id) VALUES('My Note','Hello','text','manual',200,200,200,1)",
        [],
    )
    .unwrap();
    let note_id = conn.last_insert_rowid();
    drop(conn);

    let list = send_recv(
        &mut stdin,
        &mut stdout,
        r#"{"request_id":3,"method":"list_items","params":{"folder_id":1}}"#,
    );
    assert_eq!(list["ok"], true);
    assert_eq!(list["result"]["items"][0]["id"], note_id);
    let rev = list["result"]["items"][0]["revision"]
        .as_str()
        .unwrap()
        .to_string();

    let read = send_recv(
        &mut stdin,
        &mut stdout,
        &format!(r#"{{"request_id":4,"method":"read_item","params":{{"id":{note_id}}}}}"#),
    );
    assert_eq!(read["ok"], true);
    assert_eq!(read["result"]["content"], "Hello");

    let saved = send_recv(
        &mut stdin,
        &mut stdout,
        &format!(
            r#"{{"request_id":5,"method":"save_item","params":{{"id":{note_id},"content":"Updated","expected_revision":"{rev}"}}}}"#
        ),
    );
    assert_eq!(saved["ok"], true);
    let new_rev = saved["result"]["revision"].as_str().unwrap().to_string();
    assert_ne!(new_rev, rev);

    // History item cannot be edited via bridge.
    let hist_read = send_recv(
        &mut stdin,
        &mut stdout,
        &format!(r#"{{"request_id":6,"method":"read_item","params":{{"id":{history_id}}}}}"#),
    );
    let hist_rev = hist_read["result"]["revision"].as_str().unwrap();
    let hist_save = send_recv(
        &mut stdin,
        &mut stdout,
        &format!(
            r#"{{"request_id":7,"method":"save_item","params":{{"id":{history_id},"content":"Nope","expected_revision":"{hist_rev}"}}}}"#
        ),
    );
    assert_eq!(hist_save["ok"], false);
    assert_eq!(hist_save["error"]["code"], "FORBIDDEN");

    // Close stdin -> clean exit 0.
    drop(stdin);
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
}

#[test]
fn bridge_exits_cleanly_on_sigterm() {
    let profile = TempProfile::new("sigterm");
    let mut child = spawn_bridge(&profile);
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    let ping = send_recv(
        &mut stdin,
        &mut stdout,
        r#"{"request_id":1,"method":"ping"}"#,
    );
    assert_eq!(ping["ok"], true);

    let pid = child.id() as libc::pid_t;
    assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);

    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
}
