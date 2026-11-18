//! Putting an edited value back into the text the entry was read from.
//!
//! The library is hand-written, and its files carry an order and a layout no
//! generator would reproduce: a creature's colours in the order they are
//! painted, an op to a line, the rig under them. Writing the parsed entry out
//! again would reorder every nested object, since serde_json sorts an
//! object's keys, so the page changes the one value it was asked to change
//! and leaves every byte around it alone. A one-knob edit is then a one-line
//! diff, which is what makes the page safe to use on a library somebody
//! wrote by hand.

use super::{fmt, get};
use serde_json::Value;

/// Where a value sits in the text: the byte range of the value itself (not
/// its key), and how far its key was indented.
struct At {
    start: usize,
    end: usize,
    indent: usize,
}

/// Find the value at a dotted path. Steps name object keys; a step that is
/// not there gives None.
fn find(text: &str, path: &[&str]) -> Option<At> {
    let b = text.as_bytes();
    let mut depth = 0usize;
    // How many steps of the path have been matched, and at which depth each
    // was found, so leaving that object gives the match up again.
    let mut want = 0usize;
    let mut matched: Vec<usize> = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            b'"' => {
                let open = i;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                let key = text.get(open + 1..i).unwrap_or("");
                i += 1;
                let mut j = i;
                while j < b.len() && (b[j] as char).is_whitespace() {
                    j += 1;
                }
                // A string followed by a colon is a key; one at the depth the
                // path is waiting at may be its next step.
                if j < b.len() && b[j] == b':' && want < path.len() && depth == want + 1 && key == path[want] {
                    let mut vs = j + 1;
                    while vs < b.len() && (b[vs] as char).is_whitespace() {
                        vs += 1;
                    }
                    if want + 1 == path.len() {
                        let line = text[..open].rfind('\n').map_or(0, |p| p + 1);
                        return Some(At {
                            start: vs,
                            end: value_end(text, vs)?,
                            indent: open - line,
                        });
                    }
                    want += 1;
                    matched.push(depth);
                    i = vs;
                }
            }
            b'{' | b'[' => {
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                while matched.last().is_some_and(|d| *d >= depth) {
                    matched.pop();
                    want -= 1;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// Where the value starting at `from` ends.
fn value_end(text: &str, from: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = from;
    let mut depth = 0i32;
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            b',' if depth == 0 => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// The text with the value at `path` replaced by what `doc` holds there now.
/// A path the file has not got yet is written in: the deepest part of it the
/// file does have gains the rest, whole.
pub fn put(text: &str, path: &str, doc: &Value, kind: &str) -> Result<String, String> {
    let steps: Vec<&str> = path.split('.').collect();
    let mut have = steps.len();
    while have > 0 && find(text, &steps[..have]).is_none() {
        have -= 1;
    }
    if have == steps.len() {
        let at = find(text, &steps).ok_or("not in the file")?;
        let v = get(doc, path).ok_or("gone from the entry")?;
        let mut out = String::with_capacity(text.len() + 32);
        out.push_str(&text[..at.start]);
        out.push_str(&fmt::value(v, at.indent));
        out.push_str(&text[at.end..]);
        return Ok(out);
    }
    let key = steps[have];
    let whole = steps[..have + 1].join(".");
    let v = get(doc, &whole).ok_or("gone from the entry")?;
    if have == 0 {
        Ok(add_top(text, key, v, kind))
    } else {
        let at = find(text, &steps[..have]).ok_or("not in the file")?;
        Ok(add_inside(text, &at, key, v))
    }
}

/// Write a new top-level key into the place the library's order wants it:
/// before the first key that should come after it, or at the end.
fn add_top(text: &str, key: &str, v: &Value, kind: &str) -> String {
    let line = format!("  {}: {}", Value::String(key.to_string()), fmt::value(v, 2));
    for name in fmt::after(kind, key) {
        let Some(at) = find(text, &[name]) else { continue };
        let start = text[..at.start].rfind('\n').map_or(0, |p| p + 1);
        let mut out = String::with_capacity(text.len() + line.len() + 2);
        out.push_str(&text[..start]);
        out.push_str(&line);
        out.push_str(",\n");
        out.push_str(&text[start..]);
        return out;
    }
    let close = text.rfind('}').unwrap_or(text.len());
    let before = text[..close].trim_end();
    format!("{before},\n{line}\n{}", &text[close..])
}

/// Write a new key into an object the file already has, at its end.
fn add_inside(text: &str, at: &At, key: &str, v: &Value) -> String {
    let inner = at.indent + 2;
    let body = &text[at.start..at.end];
    let close = body.rfind('}').map(|p| at.start + p).unwrap_or(at.end);
    let before = text[..close].trim_end();
    let add = format!("{}: {}", Value::String(key.to_string()), fmt::value(v, inner));
    // An object written on one line stays on one line while it still fits.
    if !body.contains('\n') {
        let out = format!("{before}, {add}{}", &text[close..]);
        let line_start = out[..close].rfind('\n').map_or(0, |p| p + 1);
        let line_end = out[line_start..].find('\n').map_or(out.len(), |p| line_start + p);
        if line_end - line_start <= fmt::WIDTH {
            return out;
        }
    }
    format!("{before},\n{}{add}\n{}{}", " ".repeat(inner), " ".repeat(at.indent), &text[close..])
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEER: &str = "{\n  \"name\": \"deer\",\n  \"height\": 1.4,\n  \"colors\": {\"coat\": [150, 98, 60], \"dark\": [36, 28, 24]},\n  \"behaviour\": {\n    \"movement\": \"graze\",\n    \"wary\": 10\n  },\n  \"ops\": [\n    {\"op\": \"box\", \"mat\": \"coat\"}\n  ]\n}\n";

    fn doc() -> Value {
        serde_json::from_str(DEER).unwrap()
    }

    #[test]
    fn one_edit_changes_one_value_and_nothing_else() {
        let mut d = doc();
        super::super::set(&mut d, "behaviour.wary", Value::from(18.0));
        let out = put(DEER, "behaviour.wary", &d, "creatures").unwrap();
        assert!(out.contains("\"wary\": 18.0"), "{out}");
        // Everything around it is untouched, the colours' own order included.
        assert!(out.contains("{\"coat\": [150, 98, 60], \"dark\": [36, 28, 24]}"), "{out}");
        assert!(out.contains("{\"op\": \"box\", \"mat\": \"coat\"}"), "{out}");
    }

    #[test]
    fn a_colour_is_repainted_in_place() {
        let mut d = doc();
        super::super::set(&mut d, "colors.coat", serde_json::json!([10, 20, 30]));
        let out = put(DEER, "colors.coat", &d, "creatures").unwrap();
        assert!(out.contains("{\"coat\": [10, 20, 30], \"dark\": [36, 28, 24]}"), "{out}");
    }

    #[test]
    fn a_field_the_file_has_not_got_is_written_in() {
        let mut d = doc();
        super::super::set(&mut d, "behaviour.flightiness", Value::from(1.5));
        let out = put(DEER, "behaviour.flightiness", &d, "creatures").unwrap();
        assert!(out.contains("\"flightiness\": 1.5"), "{out}");
        assert!(serde_json::from_str::<Value>(&out).is_ok(), "{out}");
        // A whole block the file has not got comes in with everything under it.
        let mut d = doc();
        super::super::set(&mut d, "vary.size", Value::from(0.1));
        let out = put(DEER, "vary.size", &d, "creatures").unwrap();
        let back: Value = serde_json::from_str(&out).expect("valid json");
        assert_eq!(get(&back, "vary.size").and_then(|v| v.as_f64()), Some(0.1));
    }

    #[test]
    fn a_new_top_level_key_lands_where_the_library_order_wants_it() {
        let mut d = doc();
        super::super::set(&mut d, "voxel", Value::from(0.0625));
        let out = put(DEER, "voxel", &d, "creatures").unwrap();
        let v = out.find("\"voxel\"").unwrap();
        // After the name, before the colours (libfmt's order for creatures).
        assert!(out.find("\"name\"").unwrap() < v && v < out.find("\"colors\"").unwrap(), "{out}");
        assert!(serde_json::from_str::<Value>(&out).is_ok(), "{out}");
    }
}
