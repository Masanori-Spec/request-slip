use hurl_core::{ast::{Bytes, KeyValue, MultilineStringKind, SectionValue, Template, TemplateElement}, parser::parse_hurl_file, types::ToSource};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const MAX_BYTES: usize = 262_144;
pub const MAX_ENTRIES: usize = 64;
pub const MAX_SELECTED: usize = 32;
const MAX_FIELD: usize = 8_192;
const MAX_BODY: usize = 65_536;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Pair { pub name: String, pub value: String }

#[derive(Debug, Serialize)]
pub struct RequestExport {
    pub entry: usize,
    pub source_line: usize,
    pub method: String,
    pub url: String,
    pub headers: Vec<Pair>,
    pub body_kind: &'static str,
    pub body: Option<String>,
    pub omitted_response_status: bool,
    pub argv: Vec<String>,
    pub curl_text: String,
}

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub schema: u8,
    pub parser: &'static str,
    pub source_sha256: String,
    pub source_entry_count: usize,
    pub selected_entries: Vec<usize>,
    pub scope: &'static str,
    pub requests: Vec<RequestExport>,
}

fn clean(s: &str, max: usize, multiline: bool) -> Result<(), String> {
    if s.len() > max || s.chars().any(|c| c == '\0' || (c.is_control() && !(multiline && matches!(c, '\n' | '\r' | '\t')))) {
        return Err("A literal value exceeds its limit or contains an unsupported control character".into());
    }
    Ok(())
}

fn literal(t: &Template) -> Result<String, String> {
    let mut out = String::new();
    for part in &t.elements {
        match part {
            TemplateElement::String { value, .. } => out.push_str(value),
            TemplateElement::Placeholder(_) => return Err("Templates and expressions are unsupported".into()),
        }
    }
    Ok(out)
}

fn pairs(items: &[KeyValue], headers: bool) -> Result<Vec<Pair>, String> {
    if items.len() > 64 { return Err("At most 64 pairs per section are supported".into()); }
    let mut result = Vec::new();
    for item in items {
        let name = literal(&item.key)?;
        let value = literal(&item.value)?;
        clean(&name, 256, false)?;
        clean(&value, MAX_FIELD, false)?;
        if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || b"-._~".contains(&c)) {
            return Err("Names must be nonempty ASCII letters, digits, dash, dot, underscore or tilde".into());
        }
        if headers && ["authorization", "proxy-authorization", "cookie", "set-cookie", "host", "content-length", "transfer-encoding", "connection", "expect"].contains(&name.to_ascii_lowercase().as_str()) {
            return Err("Authentication, cookie and transport-control headers are unsupported".into());
        }
        result.push(Pair { name, value });
    }
    Ok(result)
}

fn valid_url(s: &str) -> Result<(), String> {
    clean(s, MAX_FIELD, false)?;
    let tail = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).ok_or("Only absolute lowercase http:// or https:// URLs are supported")?;
    if !s.is_ascii() || s.bytes().any(|b| b <= 32 || b >= 127 || b"\\#\"<>[]{}".contains(&b)) {
        return Err("URL must be ASCII, with no fragment, backslash, brackets, braces or spaces; encode Unicode in the URL first".into());
    }
    let authority = tail.split(['/', '?']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') { return Err("URL credentials and empty hosts are unsupported".into()); }
    let mut host_port = authority.split(':');
    let host = host_port.next().unwrap_or("");
    if host.is_empty() || !host.bytes().all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c)) { return Err("Only ASCII DNS names or IPv4 hosts are supported".into()); }
    if let Some(port) = host_port.next() {
        if port.is_empty() || !port.bytes().all(|c| c.is_ascii_digit()) || port.parse::<u16>().ok().filter(|v| *v > 0).is_none() { return Err("URL port is invalid".into()); }
    }
    if host_port.next().is_some() { return Err("IPv6 URLs are outside this release".into()); }
    let bytes = s.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'%' && (i + 2 >= bytes.len() || !bytes[i + 1].is_ascii_hexdigit() || !bytes[i + 2].is_ascii_hexdigit()) { return Err("URL percent escapes must have two hexadecimal digits".into()); }
    }
    let path = tail[authority.len()..].split('?').next().unwrap_or("");
    if path.split('/').any(|p| { let p = p.to_ascii_lowercase().replace("%2e", "."); p == "." || p == ".." }) { return Err("Dot path segments are unsupported".into()); }
    Ok(())
}

fn encode_value(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { out.push(char::from(b)); }
        else { out.push_str(&format!("%{b:02X}")); }
    }
    out
}

fn encode_pairs(items: &[Pair]) -> String {
    items.iter().map(|p| format!("{}={}", p.name, encode_value(&p.value))).collect::<Vec<_>>().join("&")
}

pub fn quote_posix(s: &str) -> String { format!("'{}'", s.replace('\'', "'\"'\"'")) }

fn preflight(source: &str) -> Result<(), String> {
    if source.is_empty() || source.len() > MAX_BYTES { return Err("Input must be between 1 byte and 256 KiB".into()); }
    if source.starts_with('\u{feff}') || source.contains('<') { return Err("BOM and literal less-than signs are outside this non-XML release, including inside comments and strings".into()); }
    if source.contains("@cookie_storage") { return Err("Experimental cookie-storage directives are unsupported".into()); }
    clean(source, MAX_BYTES, true)?;
    for (_, tail) in source.match_indices("\\u{").map(|(i, _)| (i, &source[i + 3..])) {
        let digits = tail.split('}').next().unwrap_or("");
        if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) || !tail.contains('}') {
            return Err("Unicode escape-looking sequences require 1–6 hexadecimal digits and a closing brace, including inside raw text/comments".into());
        }
    }
    // Conservative raw-character budget before the recursive official parser.
    // It intentionally counts brackets inside strings/comments as well.
    if source.bytes().filter(|c| matches!(*c, b'[' | b'{')).count() > 128 { return Err("Input exceeds the conservative 128 opening-bracket budget".into()); }
    Ok(())
}

fn selection(spec: Option<&str>, count: usize) -> Result<Vec<usize>, String> {
    let chosen = match spec {
        None => (1..=count).collect::<Vec<_>>(),
        Some(s) => {
            if s.len() > 256 || s.is_empty() { return Err("Selection is empty or too long".into()); }
            let mut seen = BTreeSet::new();
            for part in s.split(',') {
                if part.is_empty() || !part.bytes().all(|c| c.is_ascii_digit()) { return Err("Use comma-separated 1-based entry numbers".into()); }
                let n: usize = part.parse().map_err(|_| "Invalid selection number")?;
                if n == 0 || n > count || !seen.insert(n) { return Err("Selection has a duplicate or out-of-range entry".into()); }
            }
            seen.into_iter().collect()
        }
    };
    if chosen.is_empty() || chosen.len() > MAX_SELECTED { return Err("Select between 1 and 32 entries".into()); }
    Ok(chosen)
}

pub fn convert(source: &str, select: Option<&str>) -> Result<Receipt, String> {
    preflight(source)?;
    let ast = parse_hurl_file(source).map_err(|e| format!("Official Hurl parser rejected input at line {}, column {}", e.pos.line, e.pos.column))?;
    if ast.entries.is_empty() || ast.entries.len() > MAX_ENTRIES { return Err("Input must contain between 1 and 64 entries".into()); }
    let selected = selection(select, ast.entries.len())?;
    let mut exports = Vec::new();
    // Validate every entry, including unselected ones. Never hide unsupported source features.
    for (index, entry) in ast.entries.iter().enumerate() {
        let make = || -> Result<RequestExport, String> {
            let req = &entry.request;
            let method = req.method.to_string();
            if !["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"].contains(&method.as_str()) { return Err("Method is outside GET, POST, PUT, PATCH, DELETE, OPTIONS".into()); }
            let mut url = literal(&req.url)?;
            valid_url(&url)?;
            let headers = pairs(&req.headers, true)?;
            let mut query = None;
            let mut form = None;
            for section in &req.sections {
                match &section.value {
                    SectionValue::QueryParams(items, _) if query.is_none() => query = Some(pairs(items, false)?),
                    SectionValue::FormParams(items, _) if form.is_none() => form = Some(pairs(items, false)?),
                    SectionValue::QueryParams(_, _) | SectionValue::FormParams(_, _) => return Err("Repeated parameter sections are unsupported".into()),
                    SectionValue::BasicAuth(_) => return Err("BasicAuth is unsupported, including an empty section".into()),
                    SectionValue::MultipartFormData(_, _) => return Err("Multipart and file uploads are unsupported".into()),
                    SectionValue::Cookies(_) => return Err("Cookies and session state are unsupported".into()),
                    SectionValue::Options(_) => return Err("Options are unsupported, including an empty section".into()),
                    SectionValue::Captures(_) | SectionValue::Asserts(_) => return Err("Captures and assertions are unsupported".into()),
                }
            }
            if let Some(response) = &entry.response {
                if !response.sections.is_empty() || !response.headers.is_empty() || response.body.is_some() { return Err("Only a bare response status line may be omitted; response sections, headers and bodies are unsupported".into()); }
            }
            if let Some(ref params) = query {
                if !params.is_empty() {
                    if !url.ends_with('?') { url.push(if url.contains('?') { '&' } else { '?' }); }
                    url.push_str(&encode_pairs(params));
                }
            }
            clean(&url, 16_384, false)?;
            let (body_kind, body) = match (&form, &req.body) {
                (Some(_), Some(_)) => return Err("Form sections cannot be combined with a body".into()),
                (Some(items), None) if !items.is_empty() => ("form", Some(encode_pairs(items))),
                (_, Some(body)) => match &body.value {
                    Bytes::OnelineString(t) => ("text", Some(literal(t)?)),
                    Bytes::MultilineString(m) => match &m.kind {
                        MultilineStringKind::Text(t) => ("text", Some(literal(t)?)),
                        MultilineStringKind::Raw(t) => {
                            // The pinned runner uses ToSource for raw bodies. Do not decode escapes.
                            if t.elements.iter().any(|e| matches!(e, TemplateElement::Placeholder(_))) { return Err("Unexpected placeholder AST in raw body".into()); }
                            ("raw", Some(t.to_source().to_string()))
                        }
                        MultilineStringKind::Json(_) | MultilineStringKind::Xml(_) | MultilineStringKind::GraphQl(_) => return Err("JSON, XML and GraphQL bodies are unsupported".into()),
                    },
                    Bytes::Json(_) | Bytes::Xml(_) | Bytes::Base64(_) | Bytes::File(_) | Bytes::Hex(_) => return Err("Structured, binary and file bodies are unsupported".into()),
                },
                _ => ("none", None),
            };
            if let Some(ref body) = body { clean(body, MAX_BODY, true)?; }
            let mut argv = ["curl", "--disable", "--globoff", "--path-as-is", "--proto", "=http,https", "--request"].map(str::to_string).to_vec();
            argv.push(method.clone());
            argv.push("--url".into()); argv.push(url.clone());
            for h in &headers {
                argv.push("--header".into());
                argv.push(if h.value.is_empty() { format!("{};", h.name) } else { format!("{}: {}", h.name, h.value) });
            }
            if !headers.iter().any(|p| p.name.eq_ignore_ascii_case("content-type")) {
                argv.push("--header".into());
                argv.push(if body_kind == "form" { "Content-Type: application/x-www-form-urlencoded" } else { "Content-Type:" }.into());
            }
            argv.extend(["--header".into(), "Expect:".into()]);
            if !headers.iter().any(|p| p.name.eq_ignore_ascii_case("user-agent")) { argv.extend(["--header".into(), "User-Agent: hurl/8.0.1".into()]); }
            if let Some(ref value) = body {
                if !value.is_empty() { argv.extend(["--data-raw".into(), value.clone()]); }
            }
            let curl_text = argv.iter().map(|s| quote_posix(s)).collect::<Vec<_>>().join(" \\\n  ");
            Ok(RequestExport { entry: index + 1, source_line: entry.source_info().start.line, method, url, headers, body_kind, body, omitted_response_status: entry.response.is_some(), argv, curl_text })
        };
        let output = make().map_err(|e| format!("Entry {}: {e}", index + 1))?;
        if selected.contains(&(index + 1)) { exports.push(output); }
    }
    let receipt = Receipt { schema: 1, parser: "hurl_core 8.0.1", source_sha256: format!("{:x}", Sha256::digest(source.as_bytes())), source_entry_count: ast.entries.len(), selected_entries: selected, scope: "Independent literal request declarations only. No requests executed. Response-derived cookies, captures, assertions, ambient options and runtime behavior are not reproduced. Review before manually running commands.", requests: exports };
    if serde_json::to_vec_pretty(&receipt).map_err(|_| "Serialization failed")?.len() + 1 > 1_048_576 { return Err("Selected receipt exceeds 1 MiB".into()); }
    Ok(receipt)
}

pub fn text_export(receipt: &Receipt) -> String {
    let mut out = String::from("# RequestSlip: POSIX cURL command text, not executed\n# Each request is independent; review before manually running.\n");
    for req in &receipt.requests { out.push_str(&format!("\n# Entry {} (source line {})\n{}\n", req.entry, req.source_line, req.curl_text)); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    const GET: &str = "GET https://fixture.invalid/hello\n";
    #[test] fn ordinary_request() { let r=convert(GET,None).unwrap(); assert_eq!(r.requests[0].method,"GET"); assert_eq!(r.selected_entries,vec![1]); }
    #[test] fn literal_pairs_preserve_duplicates_and_empty() { let r=convert("GET https://fixture.invalid/\nX-A: one\nX-A: two\nX-E:\n",None).unwrap(); assert_eq!(r.requests[0].headers.len(),3); assert!(r.requests[0].argv.contains(&"X-E;".into())); }
    #[test] fn ordered_query_encoding() { let r=convert("GET https://fixture.invalid/?a=1\n[Query]\ntag: one two\ntag: a+b\n",None).unwrap(); assert_eq!(r.requests[0].url,"https://fixture.invalid/?a=1&tag=one%20two&tag=a%2Bb"); }
    #[test] fn form_uses_literal_encoded_bytes() { let r=convert("POST https://fixture.invalid/\n[Form]\nk: =&@\nk: 日本語\n",None).unwrap(); assert_eq!(r.requests[0].body.as_deref(),Some("k=%3D%26%40&k=%E6%97%A5%E6%9C%AC%E8%AA%9E")); }
    #[test] fn raw_preserves_braces_and_backslash_n() { let r=convert("POST https://fixture.invalid/\n```raw\n@one's\\n{{raw}}\n```\n",None).unwrap(); assert_eq!(r.requests[0].body.as_deref(),Some("@one's\\n{{raw}}\n")); }
    #[test] fn text_preserves_literal_backslash_n() { let r=convert("POST https://fixture.invalid/\n```\nline\\nnext\n```\n",None).unwrap(); assert_eq!(r.requests[0].body.as_deref(),Some("line\\nnext\n")); }
    #[test] fn ordinary_placeholder_blocks() { assert!(convert("POST https://fixture.invalid/\n```\n{{variable}}\n```\n",None).is_err()); }
    #[test] fn multiline_backslash_does_not_escape_a_placeholder() { assert!(convert("POST https://fixture.invalid/\n```\n\\{{literal}}\n```\n",None).is_err()); }
    #[test] fn oneline_unicode_escape_produces_literal_braces() { let r=convert("POST https://fixture.invalid/\n`\\u{7b}{literal}}`\n",None).unwrap(); assert_eq!(r.requests[0].body.as_deref(),Some("{{literal}}")); }
    #[test] fn overlong_unicode_escape_is_rejected_before_parser() { assert!(convert("POST https://fixture.invalid/\n`\\u{123456789abcdef}`\n",None).unwrap_err().contains("Unicode escape")); }
    #[test] fn selection_keeps_source_order_and_omits_payload() { let src=format!("{GET}GET https://fixture.invalid/SECRET_UNSELECTED\n{GET}"); let r=convert(&src,Some("3,1")).unwrap(); assert_eq!(r.selected_entries,vec![1,3]); assert!(!serde_json::to_string(&r).unwrap().contains("SECRET_UNSELECTED")); }
    #[test] fn selection_errors() { for s in ["", "0", "2", "1,1", "1,a"] { assert!(convert(GET,Some(s)).is_err()); } }
    #[test] fn basic_auth_including_empty_blocks() { for s in ["[BasicAuth]\nu: p\n","[BasicAuth]\n"] { assert!(convert(&format!("{GET}{s}"),None).is_err()); } }
    #[test] fn unselected_unsupported_still_blocks() { let src=format!("{GET}{GET}[BasicAuth]\nu: p\n");assert!(convert(&src,Some("1")).is_err()); }
    #[test] fn request_sections_fail_closed() { for s in ["[Cookies]\na: b\n","[Options]\nverbose: true\n","[Multipart]\na: b\n"] { assert!(convert(&format!("{GET}{s}"),None).is_err()); } }
    #[test] fn explicit_auth_and_transport_headers_block() { for name in ["Authorization","Cookie","Host","Content-Length","Transfer-Encoding","Expect"] { assert!(convert(&format!("{GET}{name}: test\n"),None).is_err()); } }
    #[test] fn unsupported_body_kinds_block() { for body in ["{\"x\":1}","hex,4142;","base64,QQ==;","file,/tmp/never-read;","```json\n{}\n```","```xml\ntext\n```","```graphql\nquery {}\n```"] { assert!(convert(&format!("POST https://fixture.invalid/\n{body}\n"),None).is_err()); } }
    #[test] fn preparse_xml_guard_is_conservative() { for src in ["POST https://fixture.invalid/\n<a/>","GET https://fixture.invalid/\n# <comment>","POST https://fixture.invalid/\n```raw\n<x/>\n```", "POST https://fixture.invalid/\n`<x/>`"] { assert!(convert(src,None).unwrap_err().contains("less-than")); } }
    #[test] fn literal_escaped_xml_bytes_do_not_enter_xml_parser() { let r=convert("POST https://fixture.invalid/\n```raw\n\\u003cdata\\u003e\n```\n",None).unwrap(); assert_eq!(r.requests[0].body.as_deref(),Some("\\u003cdata\\u003e\n")); }
    #[test] fn directives_block_even_in_comments() { assert!(convert("# @cookie_storage_clear\nGET https://fixture.invalid/\n",None).is_err()); }
    #[test] fn source_and_controls_are_bounded() { assert!(convert(&"x".repeat(MAX_BYTES+1),None).is_err()); assert!(convert(&(GET.to_owned()+"\0"),None).is_err()); assert!(convert(&(GET.to_owned()+&"{".repeat(129)),None).is_err()); }
    #[test] fn urls_are_strict_and_credentials_block() { for url in ["file:///tmp/x","https://user:pass@fixture.invalid/","https://fixture.invalid/#x","https://fixture.invalid/a/../b","https://fixture.invalid/%xx","https://[::1]/","https://fixture.invalid/a\\b"] { assert!(convert(&format!("GET {url}\n"),None).is_err()); } }
    #[test] fn response_only_status_is_explicitly_omitted() { let r=convert(&(GET.to_owned()+"HTTP 200\n"),None).unwrap(); assert!(r.requests[0].omitted_response_status); assert!(convert(&(GET.to_owned()+"HTTP 200\n[Captures]\ntoken: header X\n"),None).is_err()); }
    #[test] fn quote_is_posix_single_argument() { assert_eq!(quote_posix("a'b $(echo x) `echo y`"),"'a'\"'\"'b $(echo x) `echo y`'"); }
    #[test] fn too_many_entries_or_default_selection_blocks() { assert!(convert(&GET.repeat(65),Some("1")).is_err()); assert!(convert(&GET.repeat(33),None).is_err()); }
    #[test] fn final_encoded_url_is_bounded() { let src=format!("{GET}[Query]\na: {}\nb: {}\n", "日本".repeat(1000), "日本".repeat(1000)); assert!(convert(&src,None).is_err()); }
    #[test] fn actual_pretty_receipt_is_bounded() { let one=format!("POST https://fixture.invalid/\n```raw\n{}\n```\n", "'".repeat(60_000)); assert!(convert(&one.repeat(3),None).unwrap_err().contains("1 MiB")); }
}
