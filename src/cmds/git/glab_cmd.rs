//! GitLab CLI (glab) command output compression.
//!
//! Focuses on high-token GitLab operations seen in history: `glab api`,
//! merge request views/lists/diffs, and mutation confirmations.

use crate::core::runner::{self, RunOptions};
use crate::core::utils::{ok_confirmation, resolved_command, truncate};
use crate::git;
use anyhow::Result;
use serde_json::Value;
use std::ffi::OsString;
use std::process::Command;

fn has_explicit_json_selector(args: &[String]) -> bool {
    args.iter()
        .any(|arg| matches!(arg.as_str(), "--jq" | "-q" | "--template"))
}

fn run_glab_json<F>(cmd: Command, label: &str, filter_fn: F) -> Result<i32>
where
    F: Fn(&Value) -> String,
{
    runner::run_filtered(
        cmd,
        "glab",
        label,
        |stdout| match serde_json::from_str::<Value>(stdout) {
            Ok(json) => filter_fn(&json),
            Err(_) => compact_text(stdout),
        },
        RunOptions::stdout_only()
            .early_exit_on_failure()
            .no_trailing_newline(),
    )
}

pub fn run(subcommand: &str, args: &[String], verbose: u8) -> Result<i32> {
    if has_explicit_json_selector(args) {
        return run_passthrough(subcommand, args, verbose);
    }

    match subcommand {
        "api" => run_api(args),
        "mr" => run_mr(args, verbose),
        "auth" | "config" => run_small_text(subcommand, args),
        _ => run_passthrough(subcommand, args, verbose),
    }
}

fn run_api(args: &[String]) -> Result<i32> {
    let mut cmd = resolved_command("glab");
    cmd.arg("api");
    for arg in args {
        cmd.arg(arg);
    }

    run_glab_json(cmd, &format!("api {}", args.join(" ")), format_api_json)
}

fn run_mr(args: &[String], verbose: u8) -> Result<i32> {
    if args.is_empty() {
        return run_passthrough("mr", args, verbose);
    }

    match args[0].as_str() {
        "diff" => mr_diff(&args[1..]),
        "view" => mr_view(&args[1..]),
        "list" => mr_list(&args[1..]),
        "create" => mr_mutation("created", args),
        "update" | "edit" => mr_mutation("updated", args),
        "merge" => mr_mutation("merged", args),
        "close" => mr_mutation("closed", args),
        "reopen" => mr_mutation("reopened", args),
        _ => run_passthrough("mr", args, verbose),
    }
}

fn mr_diff(args: &[String]) -> Result<i32> {
    let mut cmd = resolved_command("glab");
    cmd.args(["mr", "diff"]);
    for arg in args {
        cmd.arg(arg);
    }

    runner::run_filtered(
        cmd,
        "glab",
        &format!("mr diff {}", args.join(" ")),
        |raw| {
            if raw.trim().is_empty() {
                "No diff".to_string()
            } else {
                git::compact_diff(raw, 500)
            }
        },
        RunOptions::stdout_only().early_exit_on_failure(),
    )
}

fn mr_view(args: &[String]) -> Result<i32> {
    let mut cmd = resolved_command("glab");
    cmd.args(["mr", "view", "--output", "json"]);
    for arg in args {
        cmd.arg(arg);
    }

    run_glab_json(cmd, &format!("mr view {}", args.join(" ")), format_mr_value)
}

fn mr_list(args: &[String]) -> Result<i32> {
    let mut cmd = resolved_command("glab");
    cmd.args(["mr", "list", "--output", "json"]);
    for arg in args {
        cmd.arg(arg);
    }

    run_glab_json(cmd, &format!("mr list {}", args.join(" ")), |json| {
        match json.as_array() {
            Some(items) => format_mr_list(items),
            None => format_mr_value(json),
        }
    })
}

fn mr_mutation(action: &str, args: &[String]) -> Result<i32> {
    let mut cmd = resolved_command("glab");
    cmd.arg("mr");
    for arg in args {
        cmd.arg(arg);
    }
    let action = action.to_string();
    runner::run_filtered(
        cmd,
        "glab",
        &format!("mr {}", args.join(" ")),
        move |stdout| {
            if let Ok(json) = serde_json::from_str::<Value>(stdout) {
                format_mr_value(&json)
            } else {
                ok_confirmation(&action, stdout.trim())
            }
        },
        RunOptions::stdout_only().early_exit_on_failure(),
    )
}

fn run_small_text(subcommand: &str, args: &[String]) -> Result<i32> {
    let mut cmd = resolved_command("glab");
    cmd.arg(subcommand);
    for arg in args {
        cmd.arg(arg);
    }
    runner::run_filtered(
        cmd,
        "glab",
        &format!("{} {}", subcommand, args.join(" ")),
        compact_text,
        RunOptions::stdout_only().early_exit_on_failure(),
    )
}

fn format_api_json(json: &Value) -> String {
    if let Some(items) = json.as_array() {
        return format_json_array(items);
    }

    if looks_like_mr(json) {
        return format_mr_value(json);
    }

    if looks_like_project(json) {
        return format_project_value(json);
    }

    format_json_object(json)
}

fn format_json_array(items: &[Value]) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }

    let mut out = format!("{} items\n", items.len());
    for item in items.iter().take(20) {
        if looks_like_mr(item) {
            out.push_str(&format!("  {}\n", format_mr_line(item)));
        } else if looks_like_project(item) {
            out.push_str(&format!("  {}\n", format_project_line(item)));
        } else {
            out.push_str(&format!("  {}\n", summarize_json_keys(item)));
        }
    }
    if items.len() > 20 {
        out.push_str(&format!("  ... {} more\n", items.len() - 20));
    }
    out
}

fn looks_like_mr(json: &Value) -> bool {
    json.get("iid").is_some() && json.get("title").is_some() && json.get("state").is_some()
}

fn looks_like_project(json: &Value) -> bool {
    json.get("path_with_namespace").is_some() || json.get("namespace").is_some()
}

fn format_mr_list(items: &[Value]) -> String {
    let mut out = format!("Merge Requests ({})\n", items.len());
    for item in items.iter().take(20) {
        out.push_str(&format!("  {}\n", format_mr_line(item)));
    }
    if items.len() > 20 {
        out.push_str(&format!("  ... {} more\n", items.len() - 20));
    }
    out
}

fn format_mr_value(json: &Value) -> String {
    let mut out = String::new();
    out.push_str(&format!("MR {}\n", format_mr_line(json)));

    for key in [
        "source_branch",
        "target_branch",
        "merge_status",
        "detailed_merge_status",
        "has_conflicts",
        "web_url",
    ] {
        if let Some(value) = json.get(key) {
            out.push_str(&format!("  {}: {}\n", key, compact_value(value)));
        }
    }

    out.trim_end().to_string()
}

fn format_mr_line(json: &Value) -> String {
    let iid = json
        .get("iid")
        .or_else(|| json.get("id"))
        .and_then(Value::as_i64)
        .map(|value| format!("!{}", value))
        .unwrap_or_else(|| "!?".to_string());
    let state = json.get("state").and_then(Value::as_str).unwrap_or("?");
    let title = json.get("title").and_then(Value::as_str).unwrap_or("?");
    format!("{} [{}] {}", iid, state, truncate(title, 80))
}

fn format_project_value(json: &Value) -> String {
    let mut out = String::new();
    out.push_str(&format!("Project {}\n", format_project_line(json)));
    for key in [
        "id",
        "default_branch",
        "visibility",
        "last_activity_at",
        "web_url",
        "ssh_url_to_repo",
        "http_url_to_repo",
    ] {
        if let Some(value) = json.get(key) {
            out.push_str(&format!("  {}: {}\n", key, compact_value(value)));
        }
    }
    out.trim_end().to_string()
}

fn format_project_line(json: &Value) -> String {
    let path = json
        .get("path_with_namespace")
        .or_else(|| json.get("name_with_namespace"))
        .or_else(|| json.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("?");
    let id = json.get("id").and_then(Value::as_i64).unwrap_or(0);
    format!("{} [{}]", path, id)
}

fn format_json_object(json: &Value) -> String {
    let keys = json
        .as_object()
        .map(|obj| {
            obj.keys()
                .take(30)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    format!("JSON object: {{{}}}", keys)
}

fn summarize_json_keys(json: &Value) -> String {
    if let Some(obj) = json.as_object() {
        let keys = obj
            .keys()
            .take(8)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(",");
        format!("{{{}}}", keys)
    } else {
        truncate(&json.to_string(), 100)
    }
}

fn compact_value(value: &Value) -> String {
    match value {
        Value::String(value) => truncate(value, 120),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Null => "null".to_string(),
        other => truncate(&other.to_string(), 120),
    }
}

fn compact_text(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    if lines.len() <= 40 && raw.len() <= 4000 {
        return raw.trim().to_string();
    }

    let mut out = format!("{} lines, {} bytes\n", lines.len(), raw.len());
    for line in lines.iter().take(30) {
        out.push_str(&format!("  {}\n", truncate(line.trim(), 140)));
    }
    if lines.len() > 30 {
        out.push_str(&format!("  ... {} more lines\n", lines.len() - 30));
    }
    out
}

fn run_passthrough(subcommand: &str, args: &[String], verbose: u8) -> Result<i32> {
    let mut os_args: Vec<OsString> = vec![OsString::from(subcommand)];
    os_args.extend(args.iter().map(OsString::from));
    runner::run_passthrough("glab", &os_args, verbose)
}
