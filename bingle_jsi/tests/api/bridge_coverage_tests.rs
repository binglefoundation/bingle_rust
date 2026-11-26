// The native bridges and the TypeScript types must cover every `BingleJsiError` and `FailureKind`
// variant (bingle_rust #470). A variant added in Rust but missing from a bridge would reach the app
// as `Unknown`, or not compile on one platform only, so these tests read the variants from the Rust
// source and check each bridge and ts/ for them.

use std::collections::BTreeSet;
use std::path::PathBuf;

const ERRORS_BEGIN: &str = "bingle-error-kinds:begin";
const ERRORS_END: &str = "bingle-error-kinds:end";

fn read(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()))
}

/// The text after `start`, up to the first line that is exactly `end_line`.
fn block_after<'a>(text: &'a str, start: &str, end_line: &str) -> &'a str {
    let from = text
        .find(start)
        .unwrap_or_else(|| panic!("`{start}` not found"));
    let rest = &text[from + start.len()..];
    let to = rest
        .find(end_line)
        .unwrap_or_else(|| panic!("end `{end_line:?}` of `{start}` not found"));
    &rest[..to]
}

/// The text between the `bingle-error-kinds` markers.
fn marked_block<'a>(text: &'a str, file: &str) -> &'a str {
    let from = text
        .find(ERRORS_BEGIN)
        .unwrap_or_else(|| panic!("{file} has no `{ERRORS_BEGIN}` marker"));
    let to = text
        .find(ERRORS_END)
        .unwrap_or_else(|| panic!("{file} has no `{ERRORS_END}` marker"));
    assert!(from < to, "{file}: markers are out of order");
    &text[from..to]
}

/// Lines of Rust code in `block`, with comments and attributes removed.
fn code_lines(block: &str) -> impl Iterator<Item = &str> {
    block
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("//") && !line.starts_with("#["))
}

fn snake_to_camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for c in name.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn camel_to_screaming(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_uppercase());
    }
    out
}

/// `BingleJsiError`'s variants and their fields (camelCase), read from src/api/error.rs.
fn error_variants() -> Vec<(String, Vec<String>)> {
    let source = read("src/api/error.rs");
    let body = block_after(&source, "pub enum BingleJsiError {", "\n}\n");
    let mut variants: Vec<(String, Vec<String>)> = Vec::new();
    let mut open: Option<(String, Vec<String>)> = None;
    for line in code_lines(body) {
        if line.is_empty() {
            continue;
        }
        if let Some((name, mut fields)) = open.take() {
            if line.starts_with('}') {
                variants.push((name, fields));
            } else {
                let field = line
                    .split(':')
                    .next()
                    .expect("split always yields a first item");
                fields.push(snake_to_camel(field.trim()));
                open = Some((name, fields));
            }
            continue;
        }
        let Some((name, rest)) = line.split_once('{') else {
            panic!("unexpected line in BingleJsiError: {line}");
        };
        let name = name.trim().to_string();
        let rest = rest.trim();
        if let Some(inline) = rest.strip_suffix("},") {
            let fields = inline
                .split(',')
                .filter(|f| !f.trim().is_empty())
                .map(|f| {
                    snake_to_camel(
                        f.split(':')
                            .next()
                            .expect("split always yields a first item")
                            .trim(),
                    )
                })
                .collect();
            variants.push((name, fields));
        } else {
            open = Some((name, Vec::new()));
        }
    }
    assert!(open.is_none(), "unterminated variant in BingleJsiError");
    assert!(
        variants.len() >= 12,
        "parsed too few BingleJsiError variants: {variants:?}"
    );
    variants
}

/// `FailureKind`'s variants, read from src/api/types.rs.
fn failure_kinds() -> Vec<String> {
    let source = read("src/api/types.rs");
    let body = block_after(&source, "pub enum FailureKind {", "\n}\n");
    let kinds: Vec<String> = code_lines(body)
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_end_matches(',').to_string())
        .collect();
    assert!(
        kinds.len() >= 13,
        "parsed too few FailureKind variants: {kinds:?}"
    );
    kinds
}

/// Every `"kind": "X"` (or `Info("X"` / `kind: "X"`) name in a block, to catch kinds that no
/// longer exist in Rust.
fn kinds_named(block: &str, prefix: &str) -> BTreeSet<String> {
    block
        .match_indices(prefix)
        .map(|(at, _)| {
            let rest = &block[at + prefix.len()..];
            rest.split('"')
                .next()
                .expect("split always yields a first item")
                .to_string()
        })
        .collect()
}

fn expected_kinds() -> BTreeSet<String> {
    let mut kinds: BTreeSet<String> = error_variants().into_iter().map(|(name, _)| name).collect();
    kinds.insert("Unknown".to_string());
    kinds
}

#[test]
fn swift_bridge_maps_every_error_variant() {
    let source = read("ios/BingleJsiBridge.swift");
    let block = marked_block(&source, "BingleJsiBridge.swift");
    for (name, fields) in error_variants() {
        assert!(
            block.contains(&format!("case let .{name}(")),
            "BingleJsiBridge.swift bingleErrorInfo has no case for {name}"
        );
        assert!(
            block.contains(&format!("\"kind\": \"{name}\"")),
            "BingleJsiBridge.swift doesn't report kind {name}"
        );
        for field in fields {
            assert!(
                block.contains(&format!("\"{field}\":")),
                "BingleJsiBridge.swift doesn't report {name}.{field}"
            );
        }
    }
    let mut named = kinds_named(block, "\"kind\": \"");
    named.insert("Unknown".to_string());
    assert_eq!(named, expected_kinds(), "Swift kinds differ from Rust's");
    assert!(
        source.contains("return [\"kind\": \"Unknown\"]"),
        "BingleJsiBridge.swift must report other errors as Unknown"
    );
}

#[test]
fn kotlin_bridge_maps_every_error_variant() {
    let source = read("android/src/main/java/com/bingle/jsi/BingleJsiErrors.kt");
    let block = marked_block(&source, "BingleJsiErrors.kt");
    for (name, fields) in error_variants() {
        // UniFFI's Kotlin renames a variant ending in Error to end in Exception.
        let kotlin_name = match name.strip_suffix("Error") {
            Some(stem) => format!("{stem}Exception"),
            None => name.clone(),
        };
        assert!(
            block.contains(&format!("is BingleJsiException.{kotlin_name} ->")),
            "BingleJsiErrors.kt has no branch for BingleJsiException.{kotlin_name}"
        );
        assert!(
            block.contains(&format!("Info(\"{name}\"")),
            "BingleJsiErrors.kt doesn't report kind {name}"
        );
        for field in fields {
            assert!(
                block.contains(&format!("\"{field}\" to")),
                "BingleJsiErrors.kt doesn't report {name}.{field}"
            );
        }
    }
    let mut named = kinds_named(block, "Info(\"");
    named.insert("Unknown".to_string());
    assert_eq!(named, expected_kinds(), "Kotlin kinds differ from Rust's");
    assert!(
        source.contains("Info(\"Unknown\", emptyMap())"),
        "BingleJsiErrors.kt must report other errors as Unknown"
    );
}

#[test]
fn typescript_error_info_covers_every_error_variant() {
    let source = read("ts/index.ts");
    let block = marked_block(&source, "ts/index.ts");
    for (name, fields) in error_variants() {
        let start = format!("{{ kind: \"{name}\"");
        let line = block
            .lines()
            .find(|line| line.contains(&start))
            .unwrap_or_else(|| panic!("BingleJsiErrorInfo has no {name} member"));
        for field in fields {
            assert!(
                line.contains(&format!(" {field}:")),
                "BingleJsiErrorInfo {name} has no {field} field"
            );
        }
    }
    assert_eq!(
        kinds_named(block, "kind: \""),
        expected_kinds(),
        "BingleJsiErrorInfo kinds differ from Rust's"
    );
}

#[test]
fn bridges_and_typescript_cover_every_failure_kind() {
    let swift = read("ios/BingleJsiBridge.swift");
    let swift_fn = block_after(
        &swift,
        "private func failureKindToString(_ kind: FailureKind) -> String {",
        "\n    }\n",
    );
    let kotlin = read("android/src/main/java/com/bingle/jsi/BingleJsiErrors.kt");
    let kotlin_fn = block_after(
        &kotlin,
        "fun failureKindName(kind: FailureKind): String = when (kind) {",
        "\n    }\n",
    );
    let ts = read("ts/NativeBingleJsi.ts");
    let ts_union = block_after(&ts, "export type FailureKind =", ";");
    for kind in failure_kinds() {
        let mut swift_case = kind.clone();
        swift_case[..1].make_ascii_lowercase();
        assert!(
            swift_fn.contains(&format!("case .{swift_case}: return \"{kind}\"")),
            "BingleJsiBridge.swift failureKindToString doesn't map {kind}"
        );
        assert!(
            kotlin_fn.contains(&format!(
                "FailureKind.{} -> \"{kind}\"",
                camel_to_screaming(&kind)
            )),
            "BingleJsiErrors.kt failureKindName doesn't map {kind}"
        );
        assert!(
            ts_union.contains(&format!("'{kind}'")),
            "the TypeScript FailureKind union has no '{kind}'"
        );
    }
}

#[test]
fn parsers_read_the_known_variants() {
    // Guards the source parsing above: if it silently found nothing, the checks would pass.
    let variants = error_variants();
    let synced = variants
        .iter()
        .find(|(name, _)| name == "SyncedKeyWouldBeReplaced")
        .expect("SyncedKeyWouldBeReplaced is parsed");
    assert_eq!(synced.1, vec!["storedAddress".to_string()]);
    let send = variants
        .iter()
        .find(|(name, _)| name == "SendFailed")
        .expect("SendFailed is parsed");
    assert_eq!(
        send.1,
        vec!["failureKind".to_string(), "reason".to_string()]
    );
    assert!(failure_kinds().contains(&"SendBackoff".to_string()));
    assert_eq!(
        camel_to_screaming("RecipientNotAdvertised"),
        "RECIPIENT_NOT_ADVERTISED"
    );
}
