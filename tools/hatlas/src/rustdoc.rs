//! The only reader of rustdoc JSON. Every field access goes through here so a format
//! that drifts fails with the field and `format_version` named, never with a default.

use crate::Error;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// The `format_version`s a committed fixture covers.
pub const SUPPORTED: &[u32] = &[57, 61];

/// An item id. Numeric in item fields, a string as an `index` or `paths` key.
pub type Id = u64;

/// One loaded rustdoc JSON file.
pub struct Crate {
    file: PathBuf,
    version: u32,
    json: Value,
    name: String,
    root: Id,
}

/// A non-blanket, non-synthetic trait impl, reduced to the three facts classification needs.
pub struct Impl {
    pub trait_name: String,
    pub trait_id: Id,
    pub for_id: Id,
}

fn id_of(v: &Value) -> Option<Id> {
    v.as_u64()
}

impl Crate {
    /// Reads and validates one file.
    pub fn load(file: &Path) -> Result<Crate, Error> {
        let text = std::fs::read_to_string(file).map_err(|source| Error::Io {
            path: file.to_path_buf(),
            source,
        })?;
        let json = serde_json::from_str(&text).map_err(|e| Error::Metadata(format!("{}: {e}", file.display())))?;
        Crate::from_value(file.to_path_buf(), json)
    }

    /// Validates an already parsed document.
    ///
    /// Fails with `MissingField` when a required top-level field is absent and
    /// with `UnsupportedFormat` when `format_version` is not in [`SUPPORTED`].
    pub fn from_value(file: PathBuf, json: Value) -> Result<Crate, Error> {
        let missing = |version: Option<u32>, field: &str| Error::MissingField {
            file: file.clone(),
            version,
            field: field.to_string(),
        };
        let version = json
            .get("format_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| missing(None, "format_version"))? as u32;
        if !SUPPORTED.contains(&version) {
            return Err(Error::UnsupportedFormat {
                file,
                found: version,
                supported: SUPPORTED.to_vec(),
            });
        }
        let root = json
            .get("root")
            .and_then(id_of)
            .ok_or_else(|| missing(Some(version), "root"))?;
        for field in ["index", "paths", "external_crates"] {
            if !json.get(field).is_some_and(Value::is_object) {
                return Err(missing(Some(version), field));
            }
        }
        let name = json["index"]
            .get(root.to_string())
            .and_then(|r| r.get("name"))
            .and_then(Value::as_str)
            .ok_or_else(|| missing(Some(version), "index[root].name"))?
            .to_string();
        Ok(Crate { file, version, json, name, root })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn root(&self) -> Id {
        self.root
    }

    /// The item with this id.
    pub fn item(&self, id: Id) -> Result<&Value, Error> {
        self.json["index"]
            .get(id.to_string())
            .ok_or_else(|| self.missing(format!("index[{id}]")))
    }

    pub fn item_name(item: &Value) -> Option<&str> {
        item.get("name").and_then(Value::as_str)
    }

    /// `None` when the item has no rustdoc.
    pub fn docs(item: &Value) -> Option<&str> {
        item.get("docs").and_then(Value::as_str)
    }

    /// True only for `visibility == "public"`; `crate`, `default` and `restricted` are not.
    pub fn is_public(&self, item: &Value) -> Result<bool, Error> {
        let vis = item.get("visibility").ok_or_else(|| self.missing("visibility"))?;
        Ok(vis.as_str() == Some("public"))
    }

    /// The single key of `inner`: `struct`, `module`, `function` and so on.
    pub fn kind<'a>(&self, item: &'a Value) -> Result<&'a str, Error> {
        item.get("inner")
            .and_then(Value::as_object)
            .and_then(|o| o.keys().next())
            .map(String::as_str)
            .ok_or_else(|| self.missing("inner"))
    }

    pub fn module_items(&self, item: &Value) -> Result<Vec<Id>, Error> {
        item["inner"]["module"]["items"]
            .as_array()
            .map(|a| a.iter().filter_map(id_of).collect())
            .ok_or_else(|| self.missing("inner.module.items"))
    }

    /// File and begin line; `None` for items without a span.
    pub fn span(item: &Value) -> Option<(String, u64)> {
        let span = item.get("span").filter(|s| !s.is_null())?;
        let file = span.get("filename")?.as_str()?.to_string();
        let line = span.get("begin")?.get(0)?.as_u64()?;
        Some((file, line))
    }

    /// The item's intra-doc link table: the text as written, and the id it resolves to.
    pub fn links(item: &Value) -> Vec<(String, Id)> {
        item.get("links")
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| id_of(v).map(|id| (k.clone(), id)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every trait impl that is neither blanket nor synthetic, with a trait and a resolved `for` type.
    pub fn impls(&self) -> Vec<Impl> {
        self.json["index"]
            .as_object()
            .into_iter()
            .flat_map(|index| index.values())
            .filter_map(|item| {
                let imp = item.get("inner")?.get("impl")?;
                if !imp.get("blanket_impl").is_none_or(Value::is_null) {
                    return None;
                }
                if imp.get("is_synthetic").and_then(Value::as_bool) == Some(true) {
                    return None;
                }
                let tr = imp.get("trait").filter(|t| !t.is_null())?;
                let path = tr.get("path")?.as_str()?;
                let trait_name = path.rsplit("::").next()?.to_string();
                Some(Impl {
                    trait_name,
                    trait_id: id_of(tr.get("id")?)?,
                    for_id: id_of(imp.get("for")?.get("resolved_path")?.get("id")?)?,
                })
            })
            .collect()
    }

    /// The name of the crate that defines this id; `None` when `paths` does not know it.
    pub fn defining_crate(&self, id: Id) -> Option<String> {
        let crate_id = self.json["paths"].get(id.to_string())?.get("crate_id")?.as_u64()?;
        if crate_id == 0 {
            return Some(self.name.clone());
        }
        self.json["external_crates"]
            .get(crate_id.to_string())?
            .get("name")?
            .as_str()
            .map(str::to_string)
    }

    fn missing(&self, field: impl Into<String>) -> Error {
        Error::MissingField {
            file: self.file.clone(),
            version: Some(self.version),
            field: field.into(),
        }
    }
}
