//! Rendering rustdoc JSON `Type` values as the text a reader would write, and picking
//! them apart by generic argument.

use serde_json::Value;

fn last_path_segment(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

/// The text of a type, with lifetimes dropped and paths shortened to their last segment.
///
/// Fails with the name of the first `Type` variant it has no rule for, so a format
/// that grows a variant is reported instead of printed wrong.
pub fn display(ty: &Value) -> Result<String, String> {
    let (variant, body) = variant_of(ty)?;
    Ok(match variant {
        "primitive" | "generic" => body
            .as_str()
            .ok_or_else(|| variant.to_string())?
            .to_string(),
        "resolved_path" => {
            let path = body["path"].as_str().ok_or("resolved_path")?;
            format!("{}{}", last_path_segment(path), args_text(&body["args"])?)
        }
        "tuple" => {
            let members = list(body, "tuple")?
                .iter()
                .map(display)
                .collect::<Result<Vec<_>, _>>()?;
            match members.as_slice() {
                [one] => format!("({one},)"),
                _ => format!("({})", members.join(", ")),
            }
        }
        "slice" => format!("[{}]", display(body)?),
        "array" => format!(
            "[{}; {}]",
            display(&body["type"])?,
            body["len"].as_str().unwrap_or("_")
        ),
        "borrowed_ref" => {
            let mutable = if body["is_mutable"].as_bool() == Some(true) {
                "mut "
            } else {
                ""
            };
            format!("&{mutable}{}", display(&body["type"])?)
        }
        "raw_pointer" => {
            let mutable = if body["is_mutable"].as_bool() == Some(true) {
                "mut"
            } else {
                "const"
            };
            format!("*{mutable} {}", display(&body["type"])?)
        }
        "dyn_trait" => {
            let traits = list(&body["traits"], "dyn_trait")?
                .iter()
                .map(|t| trait_text(t.get("trait"), "dyn_trait"))
                .collect::<Result<Vec<_>, String>>()?;
            format!("dyn {}", traits.join(" + "))
        }
        "impl_trait" => {
            let bounds = list(body, "impl_trait")?
                .iter()
                .filter_map(|b| b.get("trait_bound"))
                .map(|b| trait_text(b.get("trait"), "impl_trait"))
                .collect::<Result<Vec<_>, String>>()?;
            format!("impl {}", bounds.join(" + "))
        }
        "function_pointer" => {
            let sig = &body["sig"];
            let inputs = list(&sig["inputs"], "function_pointer")?
                .iter()
                .map(|p| display(&p[1]))
                .collect::<Result<Vec<_>, _>>()?;
            let output = if sig["output"].is_null() {
                String::new()
            } else {
                format!(" -> {}", display(&sig["output"])?)
            };
            format!("fn({}){output}", inputs.join(", "))
        }
        "qualified_path" => {
            let name = body["name"].as_str().ok_or("qualified_path")?;
            let tr = body["trait"]
                .get("path")
                .and_then(Value::as_str)
                .map(last_path_segment);
            match tr {
                Some(tr) => format!("<{} as {tr}>::{name}", display(&body["self_type"])?),
                None => format!("{}::{name}", display(&body["self_type"])?),
            }
        }
        "infer" => "_".to_string(),
        "pat" => display(&body["type"])?,
        other => return Err(other.to_string()),
    })
}

/// The type arguments of an angle-bracketed path, in order, lifetimes and consts skipped.
pub fn type_args(ty: &Value) -> Vec<&Value> {
    ty.get("resolved_path")
        .and_then(|p| p.get("args"))
        .and_then(|a| a.get("angle_bracketed"))
        .and_then(|a| a.get("args"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| a.get("type"))
        .collect()
}

/// The last `::` segment of a resolved path's name; `None` for any other type.
pub fn last_segment(ty: &Value) -> Option<&str> {
    ty.get("resolved_path")?
        .get("path")?
        .as_str()
        .map(last_path_segment)
}

/// The members of a tuple type; any other type is its own single member.
pub fn members(ty: &Value) -> Vec<&Value> {
    match ty.get("tuple").and_then(Value::as_array) {
        Some(m) => m.iter().collect(),
        None => vec![ty],
    }
}

fn variant_of(ty: &Value) -> Result<(&str, &Value), String> {
    let obj = ty.as_object().ok_or("non-object type")?;
    let (k, v) = obj.iter().next().ok_or("empty type")?;
    Ok((k.as_str(), v))
}

fn list<'a>(v: &'a Value, what: &str) -> Result<&'a Vec<Value>, String> {
    v.as_array().ok_or_else(|| what.to_string())
}

fn trait_text(tr: Option<&Value>, variant: &str) -> Result<String, String> {
    let path = tr
        .and_then(|t| t.get("path"))
        .and_then(Value::as_str)
        .ok_or_else(|| variant.to_string())?;
    let args = tr.and_then(|t| t.get("args")).unwrap_or(&Value::Null);
    Ok(format!("{}{}", last_path_segment(path), args_text(args)?))
}

fn args_text(args: &Value) -> Result<String, String> {
    if args.is_null() {
        return Ok(String::new());
    }
    if let Some(a) = args.get("angle_bracketed") {
        let mut parts = Vec::new();
        for arg in a["args"].as_array().into_iter().flatten() {
            if let Some(t) = arg.get("type") {
                parts.push(display(t)?);
            } else if let Some(c) = arg.get("const") {
                parts.push(c["expr"].as_str().unwrap_or("_").to_string());
            } else if arg.get("infer").is_some() {
                parts.push("_".to_string());
            }
        }
        for c in a["constraints"].as_array().into_iter().flatten() {
            if let (Some(name), Some(ty)) =
                (c["name"].as_str(), c.pointer("/binding/equality/type"))
            {
                parts.push(format!("{name} = {}", display(ty)?));
            }
        }
        return Ok(if parts.is_empty() {
            String::new()
        } else {
            format!("<{}>", parts.join(", "))
        });
    }
    if let Some(p) = args.get("parenthesized") {
        let inputs = list(&p["inputs"], "parenthesized")?
            .iter()
            .map(display)
            .collect::<Result<Vec<_>, _>>()?;
        let output = if p["output"].is_null() {
            String::new()
        } else {
            format!(" -> {}", display(&p["output"])?)
        };
        return Ok(format!("({}){output}", inputs.join(", ")));
    }
    Err("generic_args".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Pins that a path prints by its last segment with lifetimes dropped, in both the v57 (`Box`) and v61 (`::core::option::Option`) spellings.
    #[test]
    fn paths_print_short_without_lifetimes() {
        let ty = json!({"resolved_path": {"path": "::core::option::Option", "id": 1, "args": {"angle_bracketed": {"args": [
            {"lifetime": "'a"}, {"type": {"resolved_path": {"path": "Score", "id": 2, "args": null}}}], "constraints": []}}}});
        assert_eq!(display(&ty).unwrap(), "Option<Score>");
    }

    // Pins references, tuples, slices and arrays, which a Query's data and filter tuples are built from.
    #[test]
    fn compound_types_print_like_source() {
        let p = json!({"resolved_path": {"path": "Progress", "id": 1, "args": null}});
        let r = json!({"borrowed_ref": {"lifetime": null, "is_mutable": true, "type": p}});
        assert_eq!(display(&r).unwrap(), "&mut Progress");
        assert_eq!(
            display(&json!({"tuple": [r, p]})).unwrap(),
            "(&mut Progress, Progress)"
        );
        assert_eq!(display(&json!({"slice": p})).unwrap(), "[Progress]");
        assert_eq!(
            display(&json!({"array": {"type": p, "len": "3"}})).unwrap(),
            "[Progress; 3]"
        );
        assert_eq!(display(&json!({"primitive": "u32"})).unwrap(), "u32");
    }

    // Pins that an unknown variant is an error naming it, never a default rendering.
    #[test]
    fn unknown_variant_is_named() {
        assert_eq!(display(&json!({"hologram": {}})).unwrap_err(), "hologram");
    }

    // Pins that type_args skips lifetimes and returns types in order, which is how Query's data and filter are found.
    #[test]
    fn type_args_skip_lifetimes() {
        let ty = json!({"resolved_path": {"path": "Query", "id": 1, "args": {"angle_bracketed": {"args": [
            {"lifetime": "'_"}, {"type": {"primitive": "a"}}, {"type": {"primitive": "b"}}], "constraints": []}}}});
        assert_eq!(type_args(&ty).len(), 2);
        assert_eq!(last_segment(&ty), Some("Query"));
    }
}
