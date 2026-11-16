use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Need {
    pub kind: String,
    pub query: Vec<(String, String)>,
    pub ttl: Duration,
}

impl Need {
    pub fn new(kind: impl Into<String>, query: Vec<(String, String)>, ttl: Duration) -> Self {
        Self {
            kind: kind.into(),
            query,
            ttl,
        }
    }
}

#[derive(Clone)]
struct Entry {
    next_try: Instant,
    fetched_ms: i64,
    value: Value,
}

#[derive(Default)]
struct Control {
    layout: Option<Value>,
    saver_playing: bool,
}

pub struct DataStore {
    host: String,
    cache: Arc<Mutex<HashMap<String, Entry>>>,
    inflight: Arc<Mutex<HashSet<String>>>,
    control: Arc<Mutex<Control>>,
}

impl DataStore {
    pub fn new(host: impl Into<String>) -> Self {
        let host = host.into().trim_end_matches('/').to_string();
        let cache = Arc::new(Mutex::new(HashMap::new()));
        let inflight = Arc::new(Mutex::new(HashSet::new()));
        let control = Arc::new(Mutex::new(Control::default()));

        // layout / screensaver 也放到后台轮询：它们绝不能卡住渲染循环。
        {
            let host = host.clone();
            let control = control.clone();
            thread::spawn(move || {
                let agent = ureq::AgentBuilder::new()
                    .timeout(Duration::from_secs(2))
                    .build();
                loop {
                    if let Ok(resp) = agent.get(&format!("{host}/api/layout")).call() {
                        if let Ok(v) = resp.into_json::<Value>() {
                            if let Ok(mut c) = control.lock() {
                                c.layout = Some(v);
                            }
                        }
                    }
                    if let Ok(resp) = agent.get(&format!("{host}/api/screensaver")).call() {
                        if let Ok(v) = resp.into_json::<Value>() {
                            let playing = v
                                .get("saver")
                                .and_then(|s| s.get("playing"))
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            if let Ok(mut c) = control.lock() {
                                c.saver_playing = playing;
                            }
                        }
                    }
                    thread::sleep(Duration::from_secs(2));
                }
            });
        }

        Self {
            host,
            cache,
            inflight,
            control,
        }
    }

    /// 只在启动时阻塞一次；运行中轮询走 `layout_snapshot`。
    pub fn layout(&self) -> anyhow::Result<Value> {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(5))
            .build();
        let v = agent
            .get(&format!("{}/api/layout", self.host))
            .call()?
            .into_json::<Value>()?;
        Ok(v)
    }

    pub fn layout_snapshot(&self) -> Option<Value> {
        self.control.lock().ok().and_then(|c| c.layout.clone())
    }

    pub fn screensaver_playing(&self) -> bool {
        self.control
            .lock()
            .map(|c| c.saver_playing)
            .unwrap_or(false)
    }

    /// 永远不阻塞：有缓存立即返回；过期时后台刷新，渲染继续用旧值。
    pub fn get(&self, need: &Need) -> Value {
        let key = key_for(need);
        let now = Instant::now();
        let cached = self
            .cache
            .lock()
            .ok()
            .and_then(|c| c.get(&key).cloned())
            .map(|e| {
                let fresh = now < e.next_try;
                (e.value, fresh)
            });

        if let Some((value, true)) = &cached {
            return value.clone();
        }

        let start = self
            .inflight
            .lock()
            .map(|mut set| {
                if set.contains(&key) {
                    false
                } else {
                    set.insert(key.clone());
                    true
                }
            })
            .unwrap_or(false);
        if start {
            spawn_fetch(
                self.host.clone(),
                need.clone(),
                key,
                self.cache.clone(),
                self.inflight.clone(),
            );
        }

        cached
            .map(|(v, _)| v)
            .unwrap_or_else(|| json!({"loading": true}))
    }
}

fn key_for(need: &Need) -> String {
    let mut q = need.query.clone();
    q.sort();
    format!(
        "{}?{}",
        need.kind,
        q.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&")
    )
}

fn url_for(host: &str, need: &Need) -> String {
    let mut qs = Vec::new();
    for (k, v) in &need.query {
        qs.push(format!(
            "{}={}",
            urlencoding::encode(k),
            urlencoding::encode(v)
        ));
    }
    if qs.is_empty() {
        format!("{host}/api/data/{}", need.kind)
    } else {
        format!("{host}/api/data/{}?{}", need.kind, qs.join("&"))
    }
}

fn spawn_fetch(
    host: String,
    need: Need,
    key: String,
    cache: Arc<Mutex<HashMap<String, Entry>>>,
    inflight: Arc<Mutex<HashSet<String>>>,
) {
    thread::spawn(move || {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(4))
            .build();
        let result: Result<Value, String> = match agent.get(&url_for(&host, &need)).call() {
            Ok(resp) => resp.into_json::<Value>().map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        let now = Instant::now();
        let fetched_ms = now_ms();
        if let Ok(mut cache) = cache.lock() {
            match result {
                Ok(mut value) => {
                    if let Some(obj) = value.as_object_mut() {
                        if !obj.contains_key("fetchedAt") {
                            obj.insert("fetchedAt".into(), json!(fetched_ms));
                        }
                    }
                    let stale = value.get("stale").and_then(Value::as_bool).unwrap_or(false);
                    let next = if stale {
                        Duration::from_secs(15)
                    } else {
                        need.ttl
                    };
                    cache.insert(
                        key.clone(),
                        Entry {
                            next_try: now + next,
                            fetched_ms,
                            value,
                        },
                    );
                }
                Err(e) => {
                    let (fetched, mut value) = cache
                        .get(&key)
                        .map(|e| (e.fetched_ms, e.value.clone()))
                        .unwrap_or((fetched_ms, json!({"error": e.clone()})));
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert("stale".into(), json!(true));
                        obj.insert("fetchedAt".into(), json!(fetched));
                        if !obj.contains_key("error") {
                            obj.insert("error".into(), json!(e));
                        }
                    }
                    cache.insert(
                        key.clone(),
                        Entry {
                            next_try: now + Duration::from_secs(15),
                            fetched_ms: fetched,
                            value,
                        },
                    );
                }
            }
        }
        if let Ok(mut set) = inflight.lock() {
            set.remove(&key);
        }
    });
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
