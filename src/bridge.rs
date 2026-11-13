use crate::{
    model::MAX_CONTENT,
    store::{BridgeError, Store},
    APP_ID, INTERFACE, PATH,
};
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_exit_signal(_sig: libc::c_int) {
    QUIT_REQUESTED.store(true, Ordering::SeqCst);
}

struct StdoutWriter {
    out: Mutex<io::BufWriter<io::Stdout>>,
}

impl StdoutWriter {
    fn new() -> Self {
        Self {
            out: Mutex::new(io::BufWriter::new(io::stdout())),
        }
    }

    fn write_value(&self, value: &Value) {
        let Ok(line) = serde_json::to_string(value) else {
            return;
        };
        if let Ok(mut guard) = self.out.lock() {
            let _ = writeln!(guard, "{line}");
            let _ = guard.flush();
        }
    }
}

fn success_response(request_id: Value, result: Value) -> Value {
    json!({
        "type": "response",
        "request_id": request_id,
        "ok": true,
        "result": result,
    })
}

fn error_response(request_id: Value, code: &str, message: &str, details: Value) -> Value {
    json!({
        "type": "response",
        "request_id": request_id,
        "ok": false,
        "error": {
            "code": code,
            "message": message,
            "details": details,
        },
    })
}

fn map_bridge_error(request_id: Value, err: BridgeError) -> Value {
    match err {
        BridgeError::NotFound(msg) => error_response(request_id, "NOT_FOUND", &msg, json!({})),
        BridgeError::ContentTooLarge {
            max_bytes,
            actual_bytes,
        } => error_response(
            request_id,
            "CONTENT_TOO_LARGE",
            "Content exceeds maximum allowed size",
            json!({
                "max_bytes": max_bytes,
                "actual_bytes": actual_bytes,
            }),
        ),
        BridgeError::InvalidContent(msg) => {
            error_response(request_id, "INVALID_CONTENT", &msg, json!({}))
        }
        BridgeError::Conflict {
            current_revision,
            title,
            content,
            updated_at,
        } => error_response(
            request_id,
            "CONFLICT",
            "Item revision does not match expected_revision",
            json!({
                "current_revision": current_revision,
                "title": title,
                "content": content,
                "updated_at": updated_at,
            }),
        ),
        BridgeError::Forbidden(msg) => error_response(request_id, "FORBIDDEN", &msg, json!({})),
        BridgeError::Internal(msg) => error_response(request_id, "INTERNAL_ERROR", &msg, json!({})),
    }
}

fn valid_request_id(value: Option<&Value>) -> Option<Value> {
    match value {
        Some(Value::String(s)) => Some(Value::String(s.clone())),
        Some(Value::Number(n)) => Some(Value::Number(n.clone())),
        _ => None,
    }
}

fn notify_changed(dbus: Option<&gio::DBusConnection>) {
    let Some(conn) = dbus else {
        return;
    };
    let _ = conn.call_sync(
        Some(APP_ID),
        PATH,
        INTERFACE,
        "NotifyChanged",
        None,
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        1000,
        gio::Cancellable::NONE,
    );
}

fn handle_request_line(
    line: &str,
    store: &mut std::result::Result<Store, String>,
    dbus: Option<&gio::DBusConnection>,
) -> Value {
    let parsed: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(err) => {
            return error_response(
                Value::Null,
                "INVALID_REQUEST",
                &format!("Malformed JSON: {err}"),
                json!({}),
            );
        }
    };
    let Some(obj) = parsed.as_object() else {
        return error_response(
            Value::Null,
            "INVALID_REQUEST",
            "Request must be a JSON object",
            json!({}),
        );
    };
    let Some(request_id) = valid_request_id(obj.get("request_id")) else {
        return error_response(
            Value::Null,
            "INVALID_REQUEST",
            "Missing or invalid request_id (expected string or number)",
            json!({}),
        );
    };
    let Some(method) = obj.get("method").and_then(Value::as_str) else {
        return error_response(
            request_id,
            "INVALID_REQUEST",
            "Missing or invalid method",
            json!({}),
        );
    };
    let params = match obj.get("params") {
        None | Some(Value::Null) => serde_json::Map::new(),
        Some(Value::Object(map)) => map.clone(),
        Some(_) => {
            return error_response(
                request_id,
                "INVALID_REQUEST",
                "params must be a JSON object",
                json!({}),
            );
        }
    };

    if method == "ping" {
        return success_response(
            request_id,
            json!({
                "version": env!("CARGO_PKG_VERSION"),
                "max_content": MAX_CONTENT,
            }),
        );
    }

    let store = match store.as_mut() {
        Ok(s) => s,
        Err(err) => {
            return error_response(request_id, "INTERNAL_ERROR", err, json!({}));
        }
    };

    match method {
        "list_folders" => {
            let include_history = match params.get("include_history") {
                None | Some(Value::Null) => false,
                Some(Value::Bool(b)) => *b,
                Some(_) => {
                    return error_response(
                        request_id,
                        "INVALID_REQUEST",
                        "include_history must be a boolean",
                        json!({}),
                    );
                }
            };
            match store.bridge_list_folders(include_history) {
                Ok(folders) => success_response(request_id, json!({ "folders": folders })),
                Err(err) => map_bridge_error(request_id, err),
            }
        }
        "list_items" => {
            let Some(folder_id) = params.get("folder_id").and_then(Value::as_i64) else {
                return error_response(
                    request_id,
                    "INVALID_REQUEST",
                    "Missing or invalid folder_id",
                    json!({}),
                );
            };
            match store.bridge_list_items(folder_id) {
                Ok(items) => success_response(request_id, json!({ "items": items })),
                Err(err) => map_bridge_error(request_id, err),
            }
        }
        "read_item" => {
            let Some(id) = params.get("id").and_then(Value::as_i64) else {
                return error_response(
                    request_id,
                    "INVALID_REQUEST",
                    "Missing or invalid id",
                    json!({}),
                );
            };
            match store.bridge_read_item(id) {
                Ok(item) => match serde_json::to_value(item) {
                    Ok(val) => success_response(request_id, val),
                    Err(err) => {
                        error_response(request_id, "INTERNAL_ERROR", &err.to_string(), json!({}))
                    }
                },
                Err(err) => map_bridge_error(request_id, err),
            }
        }
        "save_item" => {
            let Some(id) = params.get("id").and_then(Value::as_i64) else {
                return error_response(
                    request_id,
                    "INVALID_REQUEST",
                    "Missing or invalid id",
                    json!({}),
                );
            };
            let Some(content) = params.get("content").and_then(Value::as_str) else {
                return error_response(
                    request_id,
                    "INVALID_REQUEST",
                    "Missing or invalid content",
                    json!({}),
                );
            };
            let Some(expected_revision) = params.get("expected_revision").and_then(Value::as_str)
            else {
                return error_response(
                    request_id,
                    "INVALID_REQUEST",
                    "Missing or invalid expected_revision",
                    json!({}),
                );
            };
            let title = match params.get("title") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) => Some(s.as_str()),
                Some(_) => {
                    return error_response(
                        request_id,
                        "INVALID_REQUEST",
                        "title must be a string or null",
                        json!({}),
                    );
                }
            };
            match store.bridge_save_item(id, content, expected_revision, title) {
                Ok(res) => {
                    notify_changed(dbus);
                    match serde_json::to_value(res) {
                        Ok(val) => success_response(request_id, val),
                        Err(err) => error_response(
                            request_id,
                            "INTERNAL_ERROR",
                            &err.to_string(),
                            json!({}),
                        ),
                    }
                }
                Err(err) => map_bridge_error(request_id, err),
            }
        }
        _ => error_response(
            request_id,
            "INVALID_REQUEST",
            &format!("Unknown method: {method}"),
            json!({}),
        ),
    }
}

pub fn run() -> glib::ExitCode {
    QUIT_REQUESTED.store(false, Ordering::SeqCst);
    unsafe {
        libc::signal(
            libc::SIGTERM,
            handle_exit_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            handle_exit_signal as *const () as libc::sighandler_t,
        );
    }

    let writer = Arc::new(StdoutWriter::new());
    let main_context = glib::MainContext::default();
    let main_loop = glib::MainLoop::new(Some(&main_context), false);

    let loop_signal = main_loop.clone();
    glib::timeout_add_local(Duration::from_millis(25), move || {
        if QUIT_REQUESTED.load(Ordering::SeqCst) {
            loop_signal.quit();
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });

    let dbus = match gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        Ok(conn) => Some(conn),
        Err(err) => {
            eprintln!(
                "Warning: session D-Bus unavailable, continuing without live change events: {err}"
            );
            None
        }
    };

    let _subscription = dbus.as_ref().map(|conn| {
        let event_writer = Arc::clone(&writer);
        conn.subscribe_to_signal(
            Some(APP_ID),
            Some(INTERFACE),
            Some("Changed"),
            Some(PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |_signal| {
                event_writer.write_value(&json!({
                    "type": "event",
                    "event": "changed",
                    "timestamp": crate::model::now(),
                }));
            },
        )
    });

    let worker_writer = Arc::clone(&writer);
    let worker_dbus = dbus.clone();
    let worker_context = main_context.clone();
    let worker_loop = main_loop.clone();
    std::thread::spawn(move || {
        let mut store = Store::open().map_err(|e| format!("Cannot open private storage: {e}"));
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    while buf.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                        buf.pop();
                    }
                    let Ok(line) = std::str::from_utf8(&buf) else {
                        worker_writer.write_value(&error_response(
                            Value::Null,
                            "INVALID_REQUEST",
                            "Request must be valid UTF-8",
                            json!({}),
                        ));
                        continue;
                    };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let response = handle_request_line(line, &mut store, worker_dbus.as_ref());
                    worker_writer.write_value(&response);
                }
                Err(err) => {
                    eprintln!("Bridge stdin read error: {err}");
                    break;
                }
            }
        }
        worker_context.invoke(move || {
            worker_loop.quit();
        });
    });

    main_loop.run();
    glib::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::path::PathBuf;

    fn memory_store() -> std::result::Result<Store, String> {
        let mut s = Store {
            db: Connection::open_in_memory().unwrap(),
            settings: crate::model::Settings::default(),
            config_path: PathBuf::new(),
        };
        s.migrate().unwrap();
        Ok(s)
    }

    #[test]
    fn handles_malformed_and_invalid_requests() {
        let mut store = memory_store();
        let bad_json = handle_request_line("{not json", &mut store, None);
        assert_eq!(bad_json["type"], "response");
        assert_eq!(bad_json["request_id"], Value::Null);
        assert_eq!(bad_json["ok"], false);
        assert_eq!(bad_json["error"]["code"], "INVALID_REQUEST");

        let missing_id = handle_request_line(r#"{"method":"ping"}"#, &mut store, None);
        assert_eq!(missing_id["request_id"], Value::Null);
        assert_eq!(missing_id["error"]["code"], "INVALID_REQUEST");

        let unknown_method = handle_request_line(
            r#"{"request_id":42,"method":"does_not_exist"}"#,
            &mut store,
            None,
        );
        assert_eq!(unknown_method["request_id"], 42);
        assert_eq!(unknown_method["error"]["code"], "INVALID_REQUEST");
    }

    #[test]
    fn handles_ping_folders_items_read_and_save_roundtrip() {
        let mut store = memory_store();
        {
            let s = store.as_mut().unwrap();
            s.capture("Clip", "App", "", false).unwrap();
            s.save_item(None, "Note 1", "Body 1").unwrap();
        }

        let ping = handle_request_line(r#"{"request_id":"p1","method":"ping"}"#, &mut store, None);
        assert_eq!(ping["ok"], true);
        assert_eq!(ping["request_id"], "p1");
        assert_eq!(ping["result"]["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(ping["result"]["max_content"], MAX_CONTENT);

        let folders = handle_request_line(
            r#"{"request_id":1,"method":"list_folders"}"#,
            &mut store,
            None,
        );
        assert_eq!(folders["ok"], true);
        assert_eq!(folders["result"]["folders"].as_array().unwrap().len(), 1);
        assert_eq!(folders["result"]["folders"][0]["id"], 1);
        assert_eq!(folders["result"]["folders"][0]["item_count"], 1);

        let items = handle_request_line(
            r#"{"request_id":2,"method":"list_items","params":{"folder_id":1}}"#,
            &mut store,
            None,
        );
        assert_eq!(items["ok"], true);
        let item_id = items["result"]["items"][0]["id"].as_i64().unwrap();
        let rev = items["result"]["items"][0]["revision"]
            .as_str()
            .unwrap()
            .to_string();

        let read = handle_request_line(
            &format!(r#"{{"request_id":3,"method":"read_item","params":{{"id":{item_id}}}}}"#),
            &mut store,
            None,
        );
        assert_eq!(read["ok"], true);
        assert_eq!(read["result"]["content"], "Body 1");

        let save = handle_request_line(
            &format!(
                r#"{{"request_id":4,"method":"save_item","params":{{"id":{item_id},"content":"Body 2","expected_revision":"{rev}"}}}}"#
            ),
            &mut store,
            None,
        );
        assert_eq!(save["ok"], true);
        assert_ne!(save["result"]["revision"], rev);

        let conflict = handle_request_line(
            &format!(
                r#"{{"request_id":5,"method":"save_item","params":{{"id":{item_id},"content":"Body 3","expected_revision":"{rev}"}}}}"#
            ),
            &mut store,
            None,
        );
        assert_eq!(conflict["ok"], false);
        assert_eq!(conflict["error"]["code"], "CONFLICT");
        assert_eq!(
            conflict["error"]["details"]["current_revision"],
            save["result"]["revision"]
        );
        assert_eq!(conflict["error"]["details"]["content"], "Body 2");
    }
}
