#[path = "../../sourcedigger-index/tests/common/mod.rs"]
mod common;

use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct Server {
    process: Child,
    address: String,
    agent: ureq::Agent,
}

impl Server {
    fn start(data: &std::path::Path, arguments: &[&OsStr]) -> Self {
        let mut process = Command::new(env!("CARGO_BIN_EXE_sourcedigger-server"))
            .args(["--listen", "127.0.0.1:0", "--threads", "2", "--settings"])
            .arg(data.join("settings.toml"))
            .arg("--database")
            .arg(data.join("sourcedigger.sqlite"))
            .arg("--repositories")
            .arg(data.join("repositories"))
            .args(arguments)
            .stdout(Stdio::piped())
            .spawn()
            .expect("starting the server");
        let mut line = String::new();
        BufReader::new(process.stdout.take().expect("stdout is piped"))
            .read_line(&mut line)
            .expect("reading the listening line");
        let address = line
            .trim()
            .strip_prefix("Listening on ")
            .unwrap_or_else(|| panic!("unexpected first line: {line:?}"))
            .to_string();
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .into();
        Self {
            process,
            address,
            agent,
        }
    }

    fn request(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let url = format!("{}{path}", self.address);
        let mut response = match (method, body) {
            ("GET", None) => self.agent.get(&url).call(),
            ("DELETE", None) => self.agent.delete(&url).call(),
            ("POST", None) => self.agent.post(&url).send_empty(),
            ("POST", Some(body)) => self.agent.post(&url).send_json(body),
            ("PUT", Some(body)) => self.agent.put(&url).send_json(body),
            _ => panic!("unsupported request {method} {path}"),
        }
        .unwrap_or_else(|error| panic!("{method} {path}: {error}"));
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .read_json()
            .unwrap_or_else(|error| panic!("{method} {path} returned invalid JSON: {error}"));
        (status, body)
    }

    fn get(&self, path: &str) -> Value {
        let (status, body) = self.request("GET", path, None);
        assert_eq!(status, 200, "GET {path}: {body}");
        body
    }

    fn dig(&self, source: &str) -> Value {
        self.dig_from(source, "new")
    }

    fn dig_from(&self, source: &str, from: &str) -> Value {
        const TIMEOUT: Duration = Duration::from_secs(60);
        let (status, dig) = self.request(
            "POST",
            &format!("/api/sources/{source}/digs?from={from}"),
            None,
        );
        assert_eq!(status, 200, "{dig}");
        let start = Instant::now();
        loop {
            let dig = self.get(&format!("/api/digs/{}", dig["id"]));
            if dig["status"] != "queued" && dig["status"] != "running" {
                return dig;
            }
            assert!(start.elapsed() < TIMEOUT, "dig did not finish: {dig}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.process.kill().expect("killing the server");
        self.process.wait().expect("waiting for the server");
    }
}

fn summary(events: &Value) -> Vec<String> {
    events
        .as_array()
        .expect("events are a list")
        .iter()
        .map(|event| {
            format!(
                "{} {} {} {} {}",
                event["version"].as_str().unwrap(),
                event["action"].as_str().unwrap(),
                event["kind"].as_str().unwrap(),
                event["definition"]["signature"].as_str().unwrap(),
                event["definition"]["url"].as_str().unwrap(),
            )
        })
        .collect()
}

fn unused_texts(data: &std::path::Path) -> i64 {
    const UNUSED: &str = "
        SELECT (SELECT count(*) FROM signatures WHERE id NOT IN
                  (SELECT signature_id FROM events UNION SELECT previous_signature_id FROM events
                   WHERE previous_signature_id IS NOT NULL))
             + (SELECT count(*) FROM paths WHERE id NOT IN
                  (SELECT path_id FROM events UNION SELECT previous_path_id FROM events
                   WHERE previous_path_id IS NOT NULL))";
    let database =
        rusqlite::Connection::open(data.join("sourcedigger.sqlite")).expect("opening the database");
    database
        .query_row(UNUSED, [], |row| row.get(0))
        .expect("counting unused texts")
}

#[test]
fn dig_and_search() {
    let repository = tempfile::tempdir().expect("creating temporary directory");
    common::create_repository(
        repository.path(),
        &[
            (
                "v1",
                &[(
                    "a.c",
                    "int f(void) { return 0; }\nint fa;\nint h(void) { return 1; }\nEXPORT_SYMBOL(h);\n",
                )],
            ),
            (
                "v2",
                &[(
                    "a.c",
                    "long f(void) { return 0; }\nint fa;\n#define G 1\nint h(void) { return 2; }\nEXPORT_SYMBOL_GPL(h);\n",
                )],
            ),
        ],
    );
    let data = tempfile::tempdir().expect("creating temporary directory");
    let server = Server::start(data.path(), &[]);

    let settings = json!({
        "name": "fixture",
        "url": repository.path(),
        "tag_pattern": r"^(v[\d.]+)$",
        "file_pattern": r"\.[ch]$",
        "viewer_url": "https://viewer/{tag}/{path}#L{line}",
        "refresh_hours": null,
    });
    let mut invalid = settings.clone();
    invalid["tag_pattern"] = json!("(");
    let (status, body) = server.request("POST", "/api/sources", Some(invalid));
    assert_eq!(status, 400, "{body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("invalid tag pattern")
    );

    let (status, body) = server.request("POST", "/api/sources", Some(settings.clone()));
    assert_eq!(status, 200, "{body}");
    let (status, body) = server.request("POST", "/api/sources", Some(settings.clone()));
    assert_eq!(status, 400, "{body}");

    let (status, preview) = server.request(
        "POST",
        "/api/preview",
        Some(json!({"url": repository.path(), "tag_pattern": r"^(v\d+)$"})),
    );
    assert_eq!(status, 200, "{preview}");
    assert_eq!(
        preview,
        json!([{"name": "v1", "tag": "v1"}, {"name": "v2", "tag": "v2"}])
    );

    let dig = server.dig("fixture");
    assert_eq!(dig["status"], "succeeded", "{dig}");
    assert_eq!(dig["versions_done"], 2);

    let page = server.get("/api/sources/fixture/events?q=f*");
    assert_eq!(
        summary(&page["events"]),
        vec![
            "v2 signature function long f(void) https://viewer/v2/a.c#L1",
            "v1 added function int f(void) https://viewer/v1/a.c#L1",
            "v1 added variable int fa https://viewer/v1/a.c#L2",
        ]
    );
    assert_eq!(page["events"][0]["previous"]["signature"], "int f(void)");
    assert_eq!(page["events"][0]["commit"]["summary"], "v2");
    assert_eq!(page["events"][0]["commit"]["author"], "test");
    let hash = page["events"][0]["commit"]["hash"].as_str().unwrap();
    assert_eq!(hash.len(), 40);
    let in_commit = server.get(&format!(
        "/api/sources/fixture/events?q=commit:{}",
        &hash[..12]
    ));
    assert_eq!(
        summary(&in_commit["events"]),
        vec![
            "v2 added define #define G 1 https://viewer/v2/a.c#L3",
            "v2 signature function long f(void) https://viewer/v2/a.c#L1",
            "v2 modified function int h(void) https://viewer/v2/a.c#L4",
            "v2 signature invocation EXPORT_SYMBOL_GPL(h) https://viewer/v2/a.c#L5"
        ]
    );
    assert_eq!(page["next"], Value::Null);

    let first = server.get("/api/sources/fixture/events?q=*&limit=1");
    assert_eq!(
        summary(&first["events"]),
        vec!["v2 added define #define G 1 https://viewer/v2/a.c#L3"]
    );
    let rest = server.get(&format!(
        "/api/sources/fixture/events?q=*&cursor={}",
        first["next"].as_str().unwrap()
    ));
    assert_eq!(rest["events"].as_array().unwrap().len(), 7);
    let in_directory = server.get("/api/sources/fixture/events?q=path:a.");
    assert_eq!(in_directory["events"].as_array().unwrap().len(), 8);
    let in_file = server.get("/api/sources/fixture/events?q=path:a.c%20kind:function");
    assert_eq!(in_file["events"].as_array().unwrap().len(), 4);
    let elsewhere = server.get("/api/sources/fixture/symbols/f?path=b.c");
    assert_eq!(elsewhere, json!([]));
    assert_eq!(
        server
            .get("/api/sources/fixture/symbols/f?path=a.c")
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let in_first = server.get("/api/sources/fixture/events?q=before:v2");
    assert_eq!(in_first["events"].as_array().unwrap().len(), 4);
    let after_first = server.get("/api/sources/fixture/events?q=after:v1");
    assert_eq!(after_first["events"].as_array().unwrap().len(), 4);
    let only_second = server.get("/api/sources/fixture/events?q=version:v2");
    assert_eq!(only_second["events"], after_first["events"]);
    let only_first = server.get("/api/sources/fixture/events?q=version:v1%20before:v2");
    assert_eq!(only_first["events"], in_first["events"]);
    let (status, _) = server.request("GET", "/api/sources/fixture/events?q=after:v7", None);
    assert_eq!(status, 404);

    let versions = server.get("/api/sources/fixture/versions");
    assert_eq!(versions[1]["name"], "v2");
    assert_eq!(versions[1]["position"], 1);
    assert_eq!(versions[1]["added"], 1);
    assert_eq!(versions[1]["modified"], 1);
    assert_eq!(versions[1]["signature"], 2);
    let exported = server.get("/api/sources/fixture/symbols/h/code?version=v2");
    assert_eq!(exported["definitions"][1]["kind"], "invocation");
    assert_eq!(exported["definitions"][1]["code"], "EXPORT_SYMBOL_GPL(h);");
    let code = server.get("/api/sources/fixture/symbols/f/code?version=v1");
    assert_eq!(code["version"], "v1");
    assert_eq!(code["definitions"][0]["code"], "int f(void) { return 0; }");
    assert_eq!(code["definitions"][0]["url"], "https://viewer/v1/a.c#L1");
    let latest = server.get("/api/sources/fixture/symbols/G/code");
    assert_eq!(latest["definitions"][0]["code"], "#define G 1");
    let revisions = server.get("/api/sources/fixture/symbols/h/revisions");
    let codes: Vec<_> = revisions
        .as_array()
        .unwrap()
        .iter()
        .map(|revision| {
            format!(
                "{} {}",
                revision["version"], revision["definitions"][0]["code"]
            )
        })
        .collect();
    assert_eq!(
        codes,
        vec![
            "\"v1\" \"int h(void) { return 1; }\"",
            "\"v2\" \"int h(void) { return 2; }\""
        ]
    );
    let changes = server.get("/api/sources/fixture/symbols/h/changes");
    assert_eq!(changes[0]["version"], "v2");
    assert_eq!(changes[0]["previous_version"], "v1");
    assert_eq!(changes[0]["action"], "modified");
    assert_eq!(
        changes[0]["files"],
        json!([{
            "path": "a.c",
            "moved_from": null,
            "before": "int h(void) { return 1; }\n\nEXPORT_SYMBOL(h);",
            "after": "int h(void) { return 2; }\n\nEXPORT_SYMBOL_GPL(h);",
        }])
    );
    assert_eq!(changes[1]["action"], "added");
    let only_a = server.get("/api/sources/fixture/symbols/h/revisions?path=a.c");
    assert_eq!(only_a.as_array().unwrap().len(), 2);
    let elsewhere = server.get("/api/sources/fixture/symbols/h/revisions?path=b.c");
    assert_eq!(elsewhere, json!([]));
    let (status, _) = server.request(
        "GET",
        "/api/sources/fixture/symbols/f/code?version=v9",
        None,
    );
    assert_eq!(status, 404);
    let index = server.get("/api");
    assert!(index["endpoints"].as_array().unwrap().len() > 10);
    assert!(index["kinds"]["invocation"].is_string());
    let llms = server
        .agent
        .get(format!("{}/llms.txt", server.address))
        .call()
        .expect("fetching llms.txt")
        .body_mut()
        .read_to_string()
        .expect("reading llms.txt");
    assert!(llms.contains("\n- `enumerator`: "), "{llms}");
    let (status, body) = server.request("GET", "/api/sources/fixture/events?q=kind:fn", None);
    assert_eq!(status, 400, "{body}");
    assert_eq!(
        body["error"],
        "kind is one of function, define, variable, struct, union, enum, enumerator, typedef, invocation, not \"fn\""
    );
    let read = server.get("/api/sources/fixture/events?q=f*%20NOT%20path:%22a%20b%22");
    assert_eq!(
        read["query"],
        json!({"and": [
            {"term": {"field": "name", "value": "f*"}},
            {"not": {"term": {"field": "path", "value": "a b"}}},
        ]})
    );
    let count = |q: &str| {
        server.get(&format!("/api/sources/fixture/events?q={q}"))["events"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(count("-kind:invocation"), 6);
    assert_eq!(count("-action:added"), 3);
    assert_eq!(count("f*%20-fa"), 2);
    assert_eq!(count("path:(b%7Ca)%5C.c"), 8);
    assert_eq!(count("-path:a."), 0);
    assert_eq!(count("version:v1%20version:v2"), 8);
    assert_eq!(count("-version:v1"), 4);
    assert_eq!(count("kind:function%20OR%20NOT%20kind:function"), 8);
    assert_eq!(count("kind:function%20AND%20NOT%20kind:function"), 0);
    assert_eq!(count("NOT%20(version:v1%20OR%20version:v2)"), 0);
    assert_eq!(count("-after:v1"), count("version:v1"));
    assert_eq!(
        count("(f%20-action:added)%20OR%20(kind:invocation%20version:v2)"),
        count("f%20-action:added") + count("kind:invocation%20version:v2")
    );
    let filtered = server.get("/api/sources/fixture/events?q=kind:variable,define%20action:added");
    assert_eq!(filtered["events"].as_array().unwrap().len(), 2);

    assert_eq!(server.get("/api/sources/fixture/names?q=F"), json!([]));
    assert_eq!(
        server.get("/api/sources/fixture/names?q=f"),
        json!(["f", "fa"])
    );
    assert_eq!(
        server
            .get("/api/sources/fixture/symbols/f")
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let (status, _) = server.request("GET", "/api/sources/fixture/events?q=(", None);
    assert_eq!(status, 400);

    common::git(repository.path(), &["rm", "--quiet", "a.c"]);
    common::git(repository.path(), &["commit", "--quiet", "--message", "v3"]);
    common::git(repository.path(), &["tag", "v3"]);
    let dig = server.dig("fixture");
    assert_eq!(dig["status"], "succeeded", "{dig}");
    assert!(
        dig["log"]
            .as_str()
            .unwrap()
            .starts_with("3 versions found, 2 already dug, 0 dropped\nv3: 1 commits, 1 changing symbols; 0 added, 5 removed"),
        "{dig}"
    );
    let removed = server.get("/api/sources/fixture/events?q=f%20action:removed");
    assert_eq!(
        summary(&removed["events"]),
        vec!["v3 removed function long f(void) https://viewer/v2/a.c#L1"]
    );

    let overview = server.get("/api/sources/fixture");
    assert_eq!(overview["version_count"], 3);
    assert_eq!(overview["last_version"], "v3");
    assert_eq!(overview["last_dig"]["status"], "succeeded");

    std::fs::write(
        repository.path().join("late.c"),
        "int late(void) { return 1; }\n",
    )
    .expect("writing file");
    common::git(repository.path(), &["add", "--all"]);
    common::git(
        repository.path(),
        &["commit", "--quiet", "--message", "Add late"],
    );
    let dig = server.dig("fixture");
    assert!(
        dig["log"].as_str().unwrap().starts_with(
            "4 versions found, 3 already dug, 0 dropped\nunreleased: 1 commits, 1 changing symbols; 1 added"
        ),
        "{dig}"
    );
    let late = server.get("/api/sources/fixture/events?q=late");
    assert_eq!(
        summary(&late["events"]),
        vec!["unreleased added function int late(void) https://viewer/main/late.c#L1"]
    );
    assert_eq!(late["events"][0]["commit"]["summary"], "Add late");

    common::git(
        repository.path(),
        &["commit", "--quiet", "--allow-empty", "--message", "Later"],
    );
    let dig = server.dig("fixture");
    assert!(
        dig["log"].as_str().unwrap().starts_with(
            "4 versions found, 3 already dug, 1 dropped\nunreleased: 2 commits, 1 changing symbols"
        ),
        "{dig}"
    );

    std::fs::write(
        repository.path().join("gone.c"),
        "int gone(void) { return 0; }\n",
    )
    .expect("writing file");
    common::git(repository.path(), &["add", "--all"]);
    common::git(
        repository.path(),
        &["commit", "--quiet", "--message", "Add gone"],
    );
    assert_eq!(server.dig("fixture")["status"], "succeeded");
    common::git(repository.path(), &["reset", "--quiet", "--hard", "HEAD~1"]);
    assert_eq!(server.dig("fixture")["status"], "succeeded");
    assert_eq!(
        server.get("/api/sources/fixture/events?q=gone")["events"],
        json!([])
    );
    assert_eq!(unused_texts(data.path()), 0);

    common::git(repository.path(), &["tag", "v1.5", "v2"]);
    let dig = server.dig("fixture");
    assert_eq!(dig["status"], "succeeded", "{dig}");
    assert!(
        dig["log"].as_str().unwrap().starts_with(
            "5 versions found, 1 already dug, 3 dropped\nv1.5: 1 commits, 1 changing symbols; 1 added, 0 removed, 1 modified, 2 signature"
        ),
        "{dig}"
    );
    let defines = server.get("/api/sources/fixture/events?q=G");
    assert_eq!(
        summary(&defines["events"]),
        vec![
            "v3 removed define #define G 1 https://viewer/v2/a.c#L3",
            "v1.5 added define #define G 1 https://viewer/v1.5/a.c#L3"
        ]
    );
    let overview = server.get("/api/sources/fixture");
    assert_eq!(overview["version_count"], 4);
    assert_eq!(overview["last_version"], "v3");

    let dig = server.dig_from("fixture", "scratch");
    assert_eq!(dig["status"], "succeeded", "{dig}");
    assert!(
        dig["log"]
            .as_str()
            .unwrap()
            .starts_with("5 versions found, 0 already dug, 0 dropped"),
        "{dig}"
    );

    let mut retagged = settings.clone();
    retagged["tag_pattern"] = json!("^nothing$");
    let (status, body) = server.request("PUT", "/api/sources/fixture", Some(retagged));
    assert_eq!(status, 200, "{body}");
    let dig = server.dig("fixture");
    assert_eq!(dig["status"], "failed");
    assert_eq!(dig["error"], "no tag matches the tag pattern ^nothing$");

    let (status, _) = server.request("DELETE", "/api/sources/fixture", None);
    assert_eq!(status, 200);
    assert_eq!(server.get("/api/sources"), json!([]));
    assert_eq!(
        std::fs::read_dir(data.path().join("repositories"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn serves_web_interface() {
    let data = tempfile::tempdir().expect("creating temporary directory");
    let web = tempfile::tempdir().expect("creating temporary directory");
    std::fs::create_dir(web.path().join("assets")).expect("creating assets");
    std::fs::write(web.path().join("index.html"), "<html>app</html>").expect("writing index");
    std::fs::write(web.path().join("assets/app-1234.js"), "let app = 1").expect("writing asset");
    let server = Server::start(
        data.path(),
        &["--web-directory".as_ref(), web.path().as_os_str()],
    );

    let fetch = |path: &str| {
        let mut response = server
            .agent
            .get(format!("{}{path}", server.address))
            .call()
            .expect("fetching");
        let cache = response
            .headers()
            .get("cache-control")
            .map(|value| value.to_str().expect("ASCII header").to_string());
        let body = response.body_mut().read_to_string().expect("reading body");
        (response.status().as_u16(), cache, body)
    };
    assert_eq!(
        fetch("/assets/app-1234.js"),
        (
            200,
            Some("public, max-age=31536000, immutable".to_string()),
            "let app = 1".to_string()
        )
    );
    assert_eq!(
        fetch("/sources/linux/symbols/kmalloc"),
        (200, None, "<html>app</html>".to_string())
    );
    assert_eq!(fetch("/api/sources").2, "[]");
    let site: Value = serde_json::from_str(&fetch("/api/site").2).expect("site settings are JSON");
    assert_eq!(site["running"]["threads"], 2);
    assert_eq!(site["running"]["overridden"]["threads"], 2);
    assert_eq!(site["running"]["web_directory"], json!(web.path()));
    assert_eq!(site["saved"], json!({}));

    let (status, saved) = server.request("PUT", "/api/site", Some(json!({"threads": 3})));
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["saved"], json!({"threads": 3}));
    assert_eq!(
        std::fs::read_to_string(data.path().join("settings.toml")).expect("reading settings"),
        "threads = 3\n"
    );
    let (status, error) = server.request("PUT", "/api/site", Some(json!({"threads": 0})));
    assert_eq!(status, 400, "{error}");
}

#[test]
fn upgrades_an_outdated_index() {
    let data = tempfile::tempdir().expect("creating temporary directory");
    let outdated = rusqlite::Connection::open(data.path().join("sourcedigger.sqlite"))
        .expect("creating the database");
    outdated
        .execute_batch(
            "CREATE TABLE sources (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE,
                url TEXT NOT NULL, tag_pattern TEXT NOT NULL, file_pattern TEXT NOT NULL,
                viewer_url TEXT NOT NULL, refresh_hours INTEGER);
             INSERT INTO sources VALUES (1, 'old', '/nowhere', '^(v.*)$', '\\.c$', '', NULL);
             CREATE TABLE digs (id INTEGER PRIMARY KEY, source_id INTEGER NOT NULL,
                status TEXT NOT NULL, queued_at INTEGER NOT NULL, started_at INTEGER,
                finished_at INTEGER, versions_done INTEGER NOT NULL DEFAULT 0,
                versions_total INTEGER NOT NULL DEFAULT 0,
                objects_received INTEGER NOT NULL DEFAULT 0,
                objects_total INTEGER NOT NULL DEFAULT 0,
                from_scratch INTEGER NOT NULL DEFAULT 0, log TEXT NOT NULL DEFAULT '', error TEXT);
             INSERT INTO digs (source_id, status, queued_at) VALUES (1, 'running', 0);
             CREATE TABLE events (id INTEGER PRIMARY KEY, path TEXT NOT NULL);",
        )
        .expect("creating an outdated schema");
    drop(outdated);

    let server = Server::start(data.path(), &[]);
    let digs = server.get("/api/sources/old/digs");
    assert_eq!(digs.as_array().unwrap().len(), 2, "{digs}");
    assert_eq!(digs[0]["from"], "scratch", "{digs}");
    assert_eq!(
        digs[1]["error"], "the server stopped during the dig",
        "{digs}"
    );
    assert_eq!(server.get("/api/sources/old/versions"), json!([]));
}
