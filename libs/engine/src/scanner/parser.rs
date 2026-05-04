// Copyright (c) 2026 Mohamed Gouda. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Terrashift-Source-Available-1.0
// See LICENSE file in the project root for full license information.

//! Per-file HCL → typed entity extraction.
//!
//! Pattern: terrashift_plan.md §5, §11 (Terraform completeness).
//! Constitution: Article IV (loud-fail on `dynamic` blocks per clarify Q4),
//! Article XIII rule 3 (no unwrap/expect/string-slice in production).

use crate::scanner::errors::ScannerError;
use crate::scanner::inventory::*;
use std::collections::BTreeMap;
use std::path::Path;

/// Parse `content` (full file text) into a `ScannedFile`.
pub fn parse_file(path: &Path, content: &str) -> Result<ScannedFile, ScannerError> {
    let body: hcl::Body = hcl::from_str(content).map_err(|e| ScannerError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let mut sf = ScannedFile::new(path.to_path_buf());

    for structure in body.into_iter() {
        match structure {
            hcl::Structure::Block(block) => process_block(path, content, block, &mut sf)?,
            hcl::Structure::Attribute(_) => {
                // Top-level attributes outside any block — rare in real .tf;
                // capture as a note and continue.
                sf.scan_notes
                    .push("top-level attribute outside block".to_string());
            }
        }
    }

    Ok(sf)
}

fn process_block(
    path: &Path,
    content: &str,
    block: hcl::Block,
    sf: &mut ScannedFile,
) -> Result<(), ScannerError> {
    let kind = block.identifier.as_str().to_string();
    let labels: Vec<String> = block.labels.iter().map(label_to_string).collect();
    let span = approx_span(content, &kind, path);

    // Hard-fail on dynamic blocks (clarify Q4)
    if kind == "dynamic" {
        return Err(ScannerError::UnsupportedFeature {
            feature: "dynamic".to_string(),
            path: path.to_path_buf(),
            line: span.line,
            reason: "dynamic blocks require runtime expansion; deferred to Stage 5".to_string(),
        });
    }

    match kind.as_str() {
        "resource" => sf
            .resources
            .push(parse_resource(&labels, &block.body, span, path)?),
        "data" => sf
            .data_sources
            .push(parse_data_source(&labels, &block.body, span, path)?),
        "provider" => sf
            .providers
            .push(parse_provider(&labels, &block.body, span, path)?),
        "module" => sf
            .modules
            .push(parse_module(&labels, &block.body, span, path)?),
        "variable" => sf
            .variables
            .push(parse_variable(&labels, &block.body, span, path)?),
        "output" => sf
            .outputs
            .push(parse_output(&labels, &block.body, span, path)?),
        "terraform" | "locals" => {
            // Terraform settings + locals are handled in later P-NN. Note for
            // the Mapper to consider.
            sf.scan_notes
                .push(format!("{kind} block (deferred to P-05)"));
        }
        other => {
            sf.scan_notes.push(format!("unknown block kind: {other}"));
        }
    }

    // Recursively check children for nested dynamic blocks
    check_no_nested_dynamic(path, content, &block.body)?;

    // Note Stage 5 features (count, for_each, provisioners) without failing
    note_stage5_features(&block.body, sf);

    Ok(())
}

fn check_no_nested_dynamic(
    path: &Path,
    content: &str,
    body: &hcl::Body,
) -> Result<(), ScannerError> {
    for s in body.iter() {
        if let hcl::Structure::Block(b) = s {
            if b.identifier.as_str() == "dynamic" {
                let span = approx_span(content, "dynamic", path);
                return Err(ScannerError::UnsupportedFeature {
                    feature: "dynamic (nested)".to_string(),
                    path: path.to_path_buf(),
                    line: span.line,
                    reason: "nested dynamic blocks require runtime expansion; deferred to Stage 5"
                        .to_string(),
                });
            }
            check_no_nested_dynamic(path, content, &b.body)?;
        }
    }
    Ok(())
}

fn note_stage5_features(body: &hcl::Body, sf: &mut ScannedFile) {
    for attr in body.attributes() {
        let name = attr.key.as_str();
        if matches!(name, "count" | "for_each") {
            sf.scan_notes
                .push(format!("uses {name} meta-argument (Stage 5 feature)"));
        }
    }
    for s in body.iter() {
        if let hcl::Structure::Block(b) = s {
            let kind = b.identifier.as_str();
            if matches!(kind, "provisioner" | "lifecycle" | "connection") {
                sf.scan_notes
                    .push(format!("uses {kind} block (Stage 5 feature)"));
            }
        }
    }
}

fn parse_resource(
    labels: &[String],
    body: &hcl::Body,
    span: SourceSpan,
    path: &Path,
) -> Result<Resource, ScannerError> {
    if labels.len() != 2 {
        return Err(ScannerError::MalformedBlock {
            path: path.to_path_buf(),
            message: format!(
                "resource block needs 2 labels (type + name); got {}: {labels:?}",
                labels.len()
            ),
        });
    }
    Ok(Resource {
        resource_type: labels[0].clone(),
        name: labels[1].clone(),
        attributes: body_attrs_to_map(body),
        source_span: span,
    })
}

fn parse_data_source(
    labels: &[String],
    body: &hcl::Body,
    span: SourceSpan,
    path: &Path,
) -> Result<DataSource, ScannerError> {
    if labels.len() != 2 {
        return Err(ScannerError::MalformedBlock {
            path: path.to_path_buf(),
            message: format!("data block needs 2 labels; got {}", labels.len()),
        });
    }
    Ok(DataSource {
        data_type: labels[0].clone(),
        name: labels[1].clone(),
        attributes: body_attrs_to_map(body),
        source_span: span,
    })
}

fn parse_provider(
    labels: &[String],
    body: &hcl::Body,
    span: SourceSpan,
    path: &Path,
) -> Result<Provider, ScannerError> {
    if labels.is_empty() {
        return Err(ScannerError::MalformedBlock {
            path: path.to_path_buf(),
            message: "provider block needs a type label".to_string(),
        });
    }
    let attrs = body_attrs_to_map(body);
    Ok(Provider {
        provider_type: labels[0].clone(),
        alias: attrs.get("alias").cloned(),
        attributes: attrs,
        source_span: span,
    })
}

fn parse_module(
    labels: &[String],
    body: &hcl::Body,
    span: SourceSpan,
    path: &Path,
) -> Result<Module, ScannerError> {
    if labels.len() != 1 {
        return Err(ScannerError::MalformedBlock {
            path: path.to_path_buf(),
            message: format!("module block needs 1 label (name); got {}", labels.len()),
        });
    }
    let attrs = body_attrs_to_map(body);
    Ok(Module {
        name: labels[0].clone(),
        source: attrs.get("source").cloned().map(unquote),
        version: attrs.get("version").cloned().map(unquote),
        attributes: attrs,
        source_span: span,
    })
}

fn parse_variable(
    labels: &[String],
    body: &hcl::Body,
    span: SourceSpan,
    path: &Path,
) -> Result<Variable, ScannerError> {
    if labels.len() != 1 {
        return Err(ScannerError::MalformedBlock {
            path: path.to_path_buf(),
            message: format!("variable block needs 1 label; got {}", labels.len()),
        });
    }
    let attrs = body_attrs_to_map(body);
    Ok(Variable {
        name: labels[0].clone(),
        var_type: attrs.get("type").cloned(),
        default: attrs.get("default").cloned(),
        description: attrs.get("description").cloned().map(unquote),
        sensitive: attrs.get("sensitive").map(|v| v == "true").unwrap_or(false),
        source_span: span,
    })
}

fn parse_output(
    labels: &[String],
    body: &hcl::Body,
    span: SourceSpan,
    path: &Path,
) -> Result<Output, ScannerError> {
    if labels.len() != 1 {
        return Err(ScannerError::MalformedBlock {
            path: path.to_path_buf(),
            message: format!("output block needs 1 label; got {}", labels.len()),
        });
    }
    let attrs = body_attrs_to_map(body);
    Ok(Output {
        name: labels[0].clone(),
        value_expr: attrs.get("value").cloned(),
        description: attrs.get("description").cloned().map(unquote),
        sensitive: attrs.get("sensitive").map(|v| v == "true").unwrap_or(false),
        source_span: span,
    })
}

fn body_attrs_to_map(body: &hcl::Body) -> BTreeMap<String, String> {
    body.attributes()
        .map(|a| (a.key.as_str().to_string(), expr_to_string(&a.expr)))
        .collect()
}

fn expr_to_string(expr: &hcl::Expression) -> String {
    // hcl::Expression implements Display via the HCL formatter; gives back
    // the raw HCL source representation.
    expr.to_string()
}

fn label_to_string(label: &hcl::BlockLabel) -> String {
    match label {
        hcl::BlockLabel::String(s) => s.clone(),
        hcl::BlockLabel::Identifier(i) => i.as_str().to_string(),
    }
}

/// Approximate (file, line, col) for a block with the given identifier.
/// hcl-rs doesn't surface span info on Block in 0.18; we find the first
/// occurrence of `<kind> "` or `<kind> {` in the file. Cheap heuristic that's
/// usually correct for non-pathological inputs. Stage 5 will replace with
/// proper span tracking once hcl-rs exposes it.
fn approx_span(content: &str, kind: &str, path: &Path) -> SourceSpan {
    let needle_quoted = format!("{kind} \"");
    let needle_brace = format!("{kind} {{");

    let pos = content
        .find(&needle_quoted)
        .or_else(|| content.find(&needle_brace));

    match pos {
        Some(byte_offset) => {
            // Char-safe prefix lookup: `.get(..n)` returns None on UTF-8
            // boundary violations, avoiding the `&s[..n]` deny-lint
            // (Article XIII rule 3 / clippy::string_slice).
            let prefix = content.get(..byte_offset).unwrap_or("");
            let line = prefix.chars().filter(|c| *c == '\n').count() + 1;
            let col = prefix
                .rfind('\n')
                .map(|p| byte_offset - p)
                .unwrap_or(byte_offset + 1);
            SourceSpan {
                file: path.to_path_buf(),
                line,
                col,
            }
        }
        None => SourceSpan {
            file: path.to_path_buf(),
            line: 0,
            col: 0,
        },
    }
}

fn unquote(s: String) -> String {
    let trimmed = s.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        // Use char-aware slicing (deny lints reject byte-slicing strings)
        trimmed
            .chars()
            .skip(1)
            .take(trimmed.chars().count() - 2)
            .collect()
    } else {
        s
    }
}
