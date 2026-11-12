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
    fn start(arguments: &[&OsStr]) -> Self {
        let mut process = Command::new(env!("CARGO_BIN_EXE_sourcedigger-server"))
            .args(["--listen", "127.0.0.1:0"])
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
                    "int f(void) { return 0; }\nint fa;\nint h(void) { return 1; }\n",
                )],
            ),
            (
                "v2",
                &[(
                    "a.c",
                    "long f(void) { return 0; }\nint fa;\n#define G 1\nint h(void) { return 2; }\n",
                )],
            ),
        ],
    );
    let data = tempfile::tempdir().expect("creating temporary directory");
    let server = Server::start(&["--data-directory".as_ref(), data.path().as_os_str()]);

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
            "v2 modified function long f(void) https://viewer/v2/a.c#L1",
            "v1 added function int f(void) https://viewer/v1/a.c#L1",
            "v1 added variable int fa https://viewer/v1/a.c#L2",
        ]
    );
    assert_eq!(page["events"][0]["previous"]["signature"], "int f(void)");
    assert_eq!(page["next"], Value::Null);

    let first = server.get("/api/sources/fixture/events?q=*&limit=1");
    assert_eq!(
        summary(&first["events"]),
        vec!["v2 added define #define G 1 https://viewer/v2/a.c#L3"]
    );
    let rest = server.get(&format!(
        "/api/sources/fixture/events?q=*&after={}",
        first["next"].as_str().unwrap()
    ));
    assert_eq!(rest["events"].as_array().unwrap().len(), 4);
    let in_directory = server.get("/api/sources/fixture/events?q=*&path=a.");
    assert_eq!(in_directory["events"].as_array().unwrap().len(), 5);
    let in_file = server.get("/api/sources/fixture/events?q=*&path=a.c&kinds=function");
    assert_eq!(in_file["events"].as_array().unwrap().len(), 3);
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
    let in_first = server.get("/api/sources/fixture/events?q=*&until=v1");
    assert_eq!(in_first["events"].as_array().unwrap().len(), 3);
    let since_first = server.get("/api/sources/fixture/events?q=*&since=v1&until=v2");
    assert_eq!(since_first["events"].as_array().unwrap().len(), 2);
    let (status, _) = server.request("GET", "/api/sources/fixture/events?q=*&since=v7", None);
    assert_eq!(status, 404);

    let versions = server.get("/api/sources/fixture/versions");
    assert_eq!(versions[1]["name"], "v2");
    assert_eq!(versions[1]["position"], 1);
    assert_eq!(versions[1]["added"], 1);
    assert_eq!(versions[1]["modified"], 1);
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
    assert!(server.get("/api")["endpoints"].as_array().unwrap().len() > 10);
    let filtered =
        server.get("/api/sources/fixture/events?q=*&kinds=variable,define&actions=added");
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
            .starts_with("3 versions found, 2 already dug, 0 dropped\nv3: 0 added, 4 removed"),
        "{dig}"
    );
    let removed = server.get("/api/sources/fixture/events?q=f&actions=removed");
    assert_eq!(
        summary(&removed["events"]),
        vec!["v3 removed function long f(void) https://viewer/v2/a.c#L1"]
    );

    let overview = server.get("/api/sources/fixture");
    assert_eq!(overview["version_count"], 3);
    assert_eq!(overview["last_version"], "v3");
    assert_eq!(overview["last_dig"]["status"], "succeeded");

    common::git(repository.path(), &["tag", "v1.5", "v2"]);
    let dig = server.dig("fixture");
    assert_eq!(dig["status"], "succeeded", "{dig}");
    assert!(
        dig["log"].as_str().unwrap().starts_with(
            "4 versions found, 1 already dug, 2 dropped\nv1.5: 1 added, 0 removed, 1 modified"
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
    assert_eq!(server.get("/api/sources/fixture")["version_count"], 4);

    let dig = server.dig_from("fixture", "scratch");
    assert_eq!(dig["status"], "succeeded", "{dig}");
    assert!(
        dig["log"]
            .as_str()
            .unwrap()
            .starts_with("4 versions found, 0 already dug, 0 dropped"),
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
    let server = Server::start(&[
        "--data-directory".as_ref(),
        data.path().as_os_str(),
        "--web-directory".as_ref(),
        web.path().as_os_str(),
    ]);

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
}
