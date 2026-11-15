use std::collections::HashMap;
use std::time::{Duration, Instant};

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

pub struct DataStore {
    host: String,
    agent: ureq::Agent,
    cache: HashMap<String, (Instant, Value)>,
}

impl DataStore {
    pub fn new(host: impl Into<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(3))
            .build();
        Self {
            host: host.into().trim_end_matches('/').to_string(),
            agent,
            cache: HashMap::new(),
        }
    }

    pub fn layout(&self) -> anyhow::Result<Value> {
        let v = self
            .agent
            .get(&format!("{}/api/layout", self.host))
            .call()?
            .into_json::<Value>()?;
        Ok(v)
    }

    pub fn status(&self) -> anyhow::Result<Value> {
        let v = self
            .agent
            .get(&format!("{}/api/status", self.host))
            .call()?
            .into_json::<Value>()?;
        Ok(v)
    }

    pub fn screensaver(&self) -> anyhow::Result<Value> {
        let v = self
            .agent
            .get(&format!("{}/api/screensaver", self.host))
            .call()?
            .into_json::<Value>()?;
        Ok(v)
    }

    pub fn get(&mut self, need: &Need) -> Value {
        let mut query = need.query.clone();
        query.sort();
        let key = format!(
            "{}?{}",
            need.kind,
            query
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("&")
        );

        if let Some((at, val)) = self.cache.get(&key) {
            if at.elapsed() < need.ttl {
                return val.clone();
            }
        }

        let mut qs = Vec::new();
        for (k, v) in &need.query {
            qs.push(format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)));
        }
        let url = if qs.is_empty() {
            format!("{}/api/data/{}", self.host, need.kind)
        } else {
            format!("{}/api/data/{}?{}", self.host, need.kind, qs.join("&"))
        };

        match self.agent.get(&url).call() {
            Ok(resp) => match resp.into_json::<Value>() {
                Ok(mut v) => {
                    // host 的 Cache 命中时不会补 fetchedAt；组件靠它显示「更新 Ns 前」。
                    if let Some(obj) = v.as_object_mut() {
                        if !obj.contains_key("fetchedAt") {
                            obj.insert("fetchedAt".into(), json!(now_ms()));
                        }
                    }
                    self.cache.insert(key, (Instant::now(), v.clone()));
                    v
                }
                Err(e) => self.fallback(&key, format!("decode: {e}")),
            },
            Err(e) => self.fallback(&key, format!("request: {e}")),
        }
    }

    fn fallback(&self, key: &str, err: String) -> Value {
        if let Some((at, val)) = self.cache.get(key) {
            let mut v = val.clone();
            if let Some(obj) = v.as_object_mut() {
                obj.insert("stale".into(), json!(true));
                obj.insert("fetchedAt".into(), json!(now_ms() - at.elapsed().as_millis() as i64));
            }
            return v;
        }
        json!({"error": err, "stale": true})
    }
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
