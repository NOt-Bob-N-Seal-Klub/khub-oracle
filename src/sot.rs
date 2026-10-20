use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalResource {
    #[serde(rename = "URL")] pub url: String,
    #[serde(rename = "Label")] pub label: String,
    #[serde(rename = "Kind")] pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    #[serde(rename = "Name")] pub name: String,
    #[serde(rename = "Network", default)] pub network: String,
    #[serde(rename = "Purpose", default)] pub purpose: String,
    #[serde(rename = "Description", default)] pub description: String,
    #[serde(rename = "Cmd")] pub cmd: String,
    #[serde(rename = "Docs", default)] pub docs: String,
    #[serde(rename = "Verify", default)] pub verify: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoodRegistry {
    pub canonical: Vec<CanonicalResource>,
    #[serde(default)] pub commands: Vec<Command>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeprecatedResource {
    #[serde(rename = "URL")] pub url: String,
    #[serde(rename = "Label")] pub label: String,
    #[serde(rename = "Category")] pub category: String,
    #[serde(rename = "Reason", default)] pub reason: String,
    #[serde(rename = "Successor", default)] pub successor: String,
    #[serde(rename = "ArchivedOn", default)] pub archived_on: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BadRegistry {
    pub deprecated: Vec<DeprecatedResource>,
}

#[derive(Debug, Clone)]
pub struct Sot {
    pub good: GoodRegistry,
    pub bad: BadRegistry,
    pub bad_index: HashMap<String, DeprecatedResource>,
    pub good_urls: Vec<String>,
}

impl Sot {
    pub fn load(sot_dir: &str) -> anyhow::Result<Self> {
        let good_raw = std::fs::read_to_string(format!("{}/good.json", sot_dir))?;
        let bad_raw = std::fs::read_to_string(format!("{}/bad.json", sot_dir))?;
        let good: GoodRegistry = serde_json::from_str(&good_raw)?;
        let bad: BadRegistry = serde_json::from_str(&bad_raw)?;

        let mut bad_index = HashMap::new();
        for d in &bad.deprecated {
            bad_index.insert(d.url.trim_end_matches('/').to_lowercase(), d.clone());
        }
        let good_urls: Vec<String> = good.canonical.iter()
            .map(|c| c.url.trim_end_matches('/').to_lowercase())
            .collect();

        Ok(Sot { good, bad, bad_index, good_urls })
    }

    /// Classify a URL. Returns "canonical", "deprecated", or "unknown".
    pub fn classify(&self, url: &str) -> (&'static str, Option<&DeprecatedResource>) {
        let key = url.trim_end_matches('/').to_lowercase();
        if let Some(d) = self.bad_index.get(&key) {
            return ("deprecated", Some(d));
        }
        // substring match for good canonical URLs
        for good in &self.good_urls {
            if key == *good || key.starts_with(&format!("{}/", good)) {
                return ("canonical", None);
            }
        }
        ("unknown", None)
    }

    pub fn stats(&self) -> serde_json::Value {
        let mut by_kind: HashMap<&str, usize> = HashMap::new();
        for c in &self.good.canonical {
            *by_kind.entry(&c.kind).or_insert(0) += 1;
        }
        let mut by_cat: HashMap<&str, usize> = HashMap::new();
        for d in &self.bad.deprecated {
            *by_cat.entry(&d.category).or_insert(0) += 1;
        }
        serde_json::json!({
            "canonical_total": self.good.canonical.len(),
            "deprecated_total": self.bad.deprecated.len(),
            "commands_total": self.good.commands.len(),
            "canonical_by_kind": by_kind,
            "deprecated_by_category": by_cat,
        })
    }
}
