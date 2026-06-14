use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct RpcClient {
	child: Child,
	stdin: ChildStdin,
	stdout: BufReader<ChildStdout>,
	next_id: u64,
}

impl RpcClient {
	fn spawn(root: &Path) -> Self {
		let bin = env!("CARGO_BIN_EXE_mcp-fs");
		let mut child = Command::new(bin)
			.arg("--root")
			.arg(root)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.spawn()
			.expect("spawn mcp-fs");
		let stdin = child.stdin
			.take()
			.expect("stdin");
		let stdout = child.stdout
			.take()
			.expect("stdout");
		Self {
			child,
			stdin,
			stdout: BufReader::new(stdout),
			next_id: 1
		}
	}
	fn send(&mut self, method: &str, params: Value) -> Value {
		let id = self.next_id;
		self.next_id += 1;
		let req = json!({
			"jsonrpc": "2.0",
			"id": id,
			"method": method,
			"params": params
		});
		let line = serde_json::to_string(&req).expect("serialize request");
		writeln!(self.stdin, "{}", line).expect("write request");
		self.stdin
			.flush()
			.expect("flush request");
		let mut resp_line = String::new();
		loop {
			resp_line.clear();
			let bytes = self.stdout
				.read_line(&mut resp_line)
				.expect("read response");
			if bytes == 0 {
				panic!("mcp-fs exited unexpectedly");
			}
			let trimmed = resp_line.trim();
			if trimmed.is_empty() {
				continue;
			}
			let parsed: Value = match serde_json::from_str(trimmed) {
				Ok(value) => value,
				Err(_) => continue,
			};
			if parsed.get("id").and_then(Value::as_u64) == Some(id) {
				return parsed;
			}
		}
	}
}

impl Drop for RpcClient {
	fn drop(&mut self) {
		let _ = self.child.kill();
	}
}

fn write_text(path: &Path, contents: &str) {
	std::fs::create_dir_all(path.parent().unwrap()).expect("create parent");
	std::fs::write(path, contents).expect("write file");
}

fn text_lines(count: usize) -> String {
	(1..=count).map(|n| format!("line {}", n)).collect::<Vec<_>>().join("\n")
}

#[test]
fn read_file_with_start_line() {
	let root = tempfile::tempdir().expect("tempdir");
	let file = root.path().join("sample.txt");
	write_text(&file, &text_lines(6));
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send(
		"tools/call",
		json!({
			"name": "read_file",
			"arguments": { "path": "sample.txt", "start_line": 5, "limit": 2 }
		})
	);
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	assert_eq!(structured.get("count").and_then(Value::as_u64), Some(2));
	assert_eq!(structured.get("total").and_then(Value::as_u64), Some(6));
	let content = structured.get("content")
		.and_then(Value::as_str)
		.expect("content");
	assert!(content.contains("line 5"));
	assert!(content.contains("line 6"));
}

#[test]
fn read_file_limit_zero_reads_full_file() {
	let root = tempfile::tempdir().expect("tempdir");
	let file = root.path().join("full.txt");
	write_text(&file, &text_lines(4));
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send("tools/call", json!({
		"name": "read_file",
		"arguments": { "path": "full.txt", "limit": 0 }
	}));
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	assert_eq!(structured.get("count").and_then(Value::as_u64), Some(4));
	assert_eq!(structured.get("total").and_then(Value::as_u64), Some(4));
	let content = structured.get("content")
		.and_then(Value::as_str)
		.expect("content");
	assert!(content.contains("line 1"));
	assert!(content.contains("line 4"));
}

#[test]
fn read_file_empty_range_returns_code() {
	let root = tempfile::tempdir().expect("tempdir");
	let file = root.path().join("tiny.txt");
	write_text(&file, "one\ntwo\nthree");
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send(
		"tools/call",
		json!({
			"name": "read_file",
			"arguments": { "path": "tiny.txt", "start_line": 10, "limit": 5 }
		})
	);
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	assert_eq!(
		structured.get("code").and_then(Value::as_str),
		Some("EMPTY_RANGE")
	);
	let message = result.get("_meta")
		.and_then(|meta| meta.get("displayMessage"))
		.and_then(Value::as_str)
		.unwrap_or("");
	assert!(message.contains("No lines returned"));
}

#[test]
fn read_multiple_files_basic() {
	let root = tempfile::tempdir().expect("tempdir");
	let one = root.path().join("one.txt");
	let two = root.path().join("two.txt");
	write_text(&one, "alpha\nbeta");
	write_text(&two, "gamma\ndelta\nepsilon");
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send(
		"tools/call",
		json!({
			"name": "read_multiple_files",
			"arguments": { "paths": [
                { "path": "one.txt" },
                { "path": "two.txt" }
            ] }
		})
	);
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	let files = structured.get("files")
		.and_then(Value::as_array)
		.expect("files");
	assert_eq!(files.len(), 2);
}

#[test]
fn read_multiple_files_missing_file() {
	let root = tempfile::tempdir().expect("tempdir");
	let one = root.path().join("one.txt");
	write_text(&one, "alpha\nbeta");
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send(
		"tools/call",
		json!({
			"name": "read_multiple_files",
			"arguments": { "paths": [
                { "path": "one.txt" },
                { "path": "missing.txt" }
            ] }
		})
	);
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	let files = structured.get("files")
		.and_then(Value::as_array)
		.expect("files");
	let missing = files.iter()
		.find(|item| item.get("path").and_then(Value::as_str) == Some("missing.txt"))
		.expect("missing entry");
	assert_eq!(
		missing.get("code").and_then(Value::as_str),
		Some("FILE_NOT_FOUND")
	);
}

#[test]
fn read_file_binary_basic() {
	let root = tempfile::tempdir().expect("tempdir");
	let file = root.path().join("data.bin");
	std::fs::write(&file, [0u8, 1u8, 2u8, 3u8]).expect("write binary");
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send(
		"tools/call",
		json!({
			"name": "read_file_binary",
			"arguments": { "path": "data.bin", "offset": 1, "limit": 2 }
		})
	);
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	assert_eq!(structured.get("bytes_read").and_then(Value::as_u64), Some(2));
	assert_eq!(structured.get("total_bytes").and_then(Value::as_u64), Some(4));
	let content = structured.get("content_base64")
		.and_then(Value::as_str)
		.expect("content_base64");
	let expected = STANDARD.encode([1u8, 2u8]);
	assert_eq!(content, expected);
}

#[test]
fn read_file_binary_limit_zero_reads_full_file() {
	let root = tempfile::tempdir().expect("tempdir");
	let file = root.path().join("full.bin");
	std::fs::write(&file, [5u8, 6u8, 7u8]).expect("write binary");
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send("tools/call", json!({
		"name": "read_file_binary",
		"arguments": { "path": "full.bin", "limit": 0 }
	}));
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	assert_eq!(structured.get("bytes_read").and_then(Value::as_u64), Some(3));
	let content = structured.get("content_base64")
		.and_then(Value::as_str)
		.expect("content_base64");
	let expected = STANDARD.encode([5u8, 6u8, 7u8]);
	assert_eq!(content, expected);
}

#[test]
fn read_multiple_files_binary_basic() {
	let root = tempfile::tempdir().expect("tempdir");
	let one = root.path().join("one.bin");
	let two = root.path().join("two.bin");
	std::fs::write(&one, [10u8, 11u8]).expect("write one");
	std::fs::write(&two, [20u8, 21u8, 22u8]).expect("write two");
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send(
		"tools/call",
		json!({
			"name": "read_multiple_files_binary",
			"arguments": { "paths": [
                { "path": "one.bin" },
                { "path": "two.bin", "offset": 1, "limit": 1 }
            ] }
		})
	);
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	let files = structured.get("files")
		.and_then(Value::as_array)
		.expect("files");
	assert_eq!(files.len(), 2);
	let one_content = files[0].get("content_base64")
		.and_then(Value::as_str)
		.unwrap_or("");
	let two_content = files[1].get("content_base64")
		.and_then(Value::as_str)
		.unwrap_or("");
	assert_eq!(one_content, STANDARD.encode([10u8, 11u8]));
	assert_eq!(two_content, STANDARD.encode([21u8]));
}

#[test]
fn requested_scopes_ping_pong() {
	let base = tempfile::tempdir().expect("tempdir");
	let root_dir = base.path().join("root");
	let external_dir = base.path().join("external");
	std::fs::create_dir_all(&root_dir).expect("root dir");
	std::fs::create_dir_all(&external_dir).expect("external dir");
	let external_file = external_dir.join("data.txt");
	write_text(&external_file, "outside root");
	let mut client = RpcClient::spawn(&root_dir);
	let rel_to_external = PathBuf::from("..")
		.join("external")
		.join("data.txt")
		.to_string_lossy()
		.to_string();
	let resp = client.send("tools/call", json!({
		"name": "read_file",
		"arguments": { "path": rel_to_external }
	}));
	let meta = resp.get("result")
		.and_then(|result| result.get("_meta"))
		.expect("meta");
	let scopes = meta.get("requested_scopes")
		.and_then(Value::as_array)
		.expect("requested_scopes");
	assert!(!scopes.is_empty());
	let scope = scopes[0].as_str()
		.expect("scope string")
		.to_string();
	let resp = client.send(
		"tools/call",
		json!({
			"name": "read_file",
			"arguments": { "path": "../external/data.txt" },
			"_meta": { "allowed_scopes": [scope] }
		})
	);
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	let content = structured.get("content")
		.and_then(Value::as_str)
		.expect("content");
	assert!(content.contains("outside root"));
}

#[test]
fn write_file_preview_and_apply() {
	let root = tempfile::tempdir().expect("tempdir");
	let file = root.path().join("write.txt");
	write_text(&file, "alpha");
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send(
		"tools/call",
		json!({
			"name": "write_file",
			"arguments": { "path": "write.txt", "content": "beta", "mode": "overwrite" },
			"_meta": { "preview": true }
		})
	);
	let result = resp.get("result").expect("result");
	let meta = result.get("_meta").expect("meta");
	assert_eq!(
		meta.get("displayMessage").and_then(Value::as_str),
		Some("Preview generated for write.txt. No changes were applied.")
	);
	assert_eq!(
		meta.get("ui")
		.and_then(|ui| ui.get("resourceUri"))
		.and_then(Value::as_str),
		Some("ui://review/index.html")
	);
	let content = result.get("content")
		.and_then(Value::as_array)
		.expect("content");
	let text = content.get(0)
		.and_then(|item| item.get("text"))
		.and_then(Value::as_str)
		.expect("content text");
	assert!(text.contains("Wrote"));
	let structured = result.get("structuredContent").expect("structured");
	assert_eq!(structured.get("path").and_then(Value::as_str), Some("write.txt"));
	assert_eq!(structured.get("original").and_then(Value::as_str), Some("alpha"));
	assert_eq!(structured.get("new").and_then(Value::as_str), Some("beta"));
	let current = std::fs::read_to_string(&file).expect("read file");
	assert_eq!(current, "alpha");
	let resp = client.send(
		"tools/call",
		json!({
			"name": "write_file",
			"arguments": { "path": "write.txt", "content": "beta", "mode": "overwrite" }
		})
	);
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	assert_eq!(
		structured.get("path").and_then(Value::as_str),
		Some("write.txt")
	);
	let current = std::fs::read_to_string(&file).expect("read file");
	assert_eq!(current, "beta");
}

#[test]
fn write_file_review_on_apply_default() {
	let root = tempfile::tempdir().expect("tempdir");
	let file = root.path().join("review.txt");
	write_text(&file, "alpha");
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send(
		"tools/call",
		json!({
			"name": "write_file",
			"arguments": { "path": "review.txt", "content": "beta", "mode": "overwrite" }
		})
	);
	let result = resp.get("result").expect("result");
	let meta = result.get("_meta").expect("meta");
	assert!(meta.get("displayMessage").and_then(Value::as_str).is_some());
	assert_eq!(
		meta.get("ui")
		.and_then(|ui| ui.get("resourceUri"))
		.and_then(Value::as_str),
		Some("ui://review/index.html")
	);
	let structured = result.get("structuredContent").expect("structured");
	assert_eq!(structured.get("original").and_then(Value::as_str), Some("alpha"));
	assert_eq!(structured.get("new").and_then(Value::as_str), Some("beta"));
	let current = std::fs::read_to_string(&file).expect("read file");
	assert_eq!(current, "beta");
}

#[test]
fn write_file_review_on_apply_disabled() {
	let root = tempfile::tempdir().expect("tempdir");
	let file = root.path().join("review-disabled.txt");
	write_text(&file, "alpha");
	let mut client = RpcClient::spawn(root.path());
	client.send("initialize", json!({
		"capabilities": {
                "experimental": {
                    "configuration": {
                        "review_on_apply": false
                    }
                }
            }
	}));
	let resp = client.send(
		"tools/call",
		json!({
			"name": "write_file",
			"arguments": { "path": "review-disabled.txt", "content": "beta", "mode": "overwrite" }
		})
	);
	let result = resp.get("result").expect("result");
	let meta = result.get("_meta").expect("meta");
	assert!(meta.get("displayMessage").and_then(Value::as_str).is_some());
	assert!(meta.get("ui").is_none());
	let current = std::fs::read_to_string(&file).expect("read file");
	assert_eq!(current, "beta");
}

#[test]
fn write_file_review_on_apply_policy_override() {
	let root = tempfile::tempdir().expect("tempdir");
	let file = root.path().join("review-policy.txt");
	write_text(&file, "alpha");
	let mut client = RpcClient::spawn(root.path());
	client.send("initialize", json!({
		"capabilities": {
                "experimental": {
                    "configuration": {
                        "review_on_apply": false
                    }
                }
            }
	}));
	let resp = client.send(
		"tools/call",
		json!({
			"name": "write_file",
			"arguments": { "path": "review-policy.txt", "content": "beta", "mode": "overwrite" },
			"_meta": {
                "policy": {
                    "review_on_apply": true
                }
            }
		})
	);
	let result = resp.get("result").expect("result");
	let meta = result.get("_meta").expect("meta");
	assert!(meta.get("displayMessage").and_then(Value::as_str).is_some());
	assert_eq!(
		meta.get("ui")
		.and_then(|ui| ui.get("resourceUri"))
		.and_then(Value::as_str),
		Some("ui://review/index.html")
	);
	let current = std::fs::read_to_string(&file).expect("read file");
	assert_eq!(current, "beta");
}

#[test]
fn edit_file_preview_and_apply() {
	let root = tempfile::tempdir().expect("tempdir");
	let file = root.path().join("edit.txt");
	write_text(&file, "one\ntwo\nthree");
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send(
		"tools/call",
		json!({
			"name": "edit_file",
			"arguments": {
                "path": "edit.txt",
                "edits": [
                    { "find": "two", "replace": "TWO" }
                ]
            },
			"_meta": { "preview": true }
		})
	);
	let result = resp.get("result").expect("result");
	let meta = result.get("_meta").expect("meta");
	assert!(meta.get("displayMessage").and_then(Value::as_str).is_some());
	assert_eq!(
		meta.get("ui")
		.and_then(|ui| ui.get("resourceUri"))
		.and_then(Value::as_str),
		Some("ui://review/index.html")
	);
	let structured = result.get("structuredContent").expect("structured");
	assert_eq!(
		structured.get("original").and_then(Value::as_str),
		Some("one\ntwo\nthree")
	);
	assert_eq!(
		structured.get("new").and_then(Value::as_str),
		Some("one\nTWO\nthree")
	);
	let current = std::fs::read_to_string(&file).expect("read file");
	assert!(current.contains("two"));
	let resp = client.send(
		"tools/call",
		json!({
			"name": "edit_file",
			"arguments": {
                "path": "edit.txt",
                "edits": [
                    { "find": "two", "replace": "TWO" }
                ]
            }
		})
	);
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	assert_eq!(
		structured.get("path").and_then(Value::as_str),
		Some("edit.txt")
	);
	let current = std::fs::read_to_string(&file).expect("read file");
	assert!(current.contains("TWO"));
}

#[test]
fn write_file_append_and_prepend() {
	let root = tempfile::tempdir().expect("tempdir");
	let file = root.path().join("append.txt");
	write_text(&file, "one");
	let mut client = RpcClient::spawn(root.path());
	let _ = client.send(
		"tools/call",
		json!({
			"name": "write_file",
			"arguments": { "path": "append.txt", "content": "two", "mode": "append" }
		})
	);
	let current = std::fs::read_to_string(&file).expect("read file");
	assert_eq!(current, "one\ntwo");
	let _ = client.send(
		"tools/call",
		json!({
			"name": "write_file",
			"arguments": { "path": "append.txt", "content": "zero", "mode": "prepend" }
		})
	);
	let current = std::fs::read_to_string(&file).expect("read file");
	assert_eq!(current, "zero\none\ntwo");
}

#[test]
fn write_file_scope_grant_ping_pong() {
	let base = tempfile::tempdir().expect("tempdir");
	let root_dir = base.path().join("root");
	let external_dir = base.path().join("external");
	std::fs::create_dir_all(&root_dir).expect("root dir");
	std::fs::create_dir_all(&external_dir).expect("external dir");
	let external_file = external_dir.join("data.txt");
	write_text(&external_file, "before");
	let mut client = RpcClient::spawn(&root_dir);
	let resp = client.send(
		"tools/call",
		json!({
			"name": "write_file",
			"arguments": { "path": "../external/data.txt", "content": "after", "mode": "overwrite" }
		})
	);
	let meta = resp.get("result")
		.and_then(|result| result.get("_meta"))
		.expect("meta");
	let scopes = meta.get("requested_scopes")
		.and_then(Value::as_array)
		.expect("requested_scopes");
	let scope = scopes[0].as_str()
		.expect("scope string")
		.to_string();
	let _ = client.send(
		"tools/call",
		json!({
			"name": "write_file",
			"arguments": { "path": "../external/data.txt", "content": "after", "mode": "overwrite" },
			"_meta": { "allowed_scopes": [scope] }
		})
	);
	let current = std::fs::read_to_string(&external_file).expect("read file");
	assert_eq!(current, "after");
}

#[test]
fn list_roots_includes_allowed_scopes() {
	let base = tempfile::tempdir().expect("tempdir");
	let root_dir = base.path().join("root");
	let external_dir = base.path().join("external");
	std::fs::create_dir_all(&root_dir).expect("root dir");
	std::fs::create_dir_all(&external_dir).expect("external dir");
	let mut client = RpcClient::spawn(&root_dir);
	let resp = client.send("tools/call", json!({
		"name": "list_roots",
		"arguments": {}
	}));
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	let roots = structured.get("roots")
		.and_then(Value::as_array)
		.expect("roots");
	let default_root = root_dir.to_string_lossy().to_string();
	let default_entry = roots.iter()
		.find(|item| item.get("path").and_then(Value::as_str) == Some(&default_root))
		.expect("default root entry");
	assert_eq!(
		default_entry.get("default").and_then(Value::as_bool),
		Some(true)
	);
	let granted_scope = format!("read:file:{}", external_dir.to_string_lossy());
	let resp = client.send(
		"tools/call",
		json!({
			"name": "list_roots",
			"arguments": {},
			"_meta": { "allowed_scopes": [granted_scope] }
		})
	);
	let result = resp.get("result").expect("result");
	let structured = result.get("structuredContent").expect("structured");
	let roots = structured.get("roots")
		.and_then(Value::as_array)
		.expect("roots");
	let external_entry = roots.iter()
		.find(
			|item| {
				item.get("path").and_then(Value::as_str) == Some(external_dir.to_string_lossy().as_ref())
			})
		.expect("external root entry");
	assert_eq!(
		external_entry.get("default").and_then(Value::as_bool),
		Some(false)
	);
}

#[test]
fn move_file_and_delete_directory() {
	let root = tempfile::tempdir().expect("tempdir");
	let dir = root.path().join("src");
	let nested = dir.join("nested");
	std::fs::create_dir_all(&nested).expect("nested dir");
	write_text(&nested.join("file.txt"), "data");
	let mut client = RpcClient::spawn(root.path());
	let _ = client.send("tools/call", json!({
		"name": "move_file",
		"arguments": { "from": "src", "to": "dst" }
	}));
	assert!(root.path().join("dst").exists());
	assert!(root.path().join("dst/nested/file.txt").exists());
	let _ = client.send("tools/call", json!({
		"name": "delete_file",
		"arguments": { "paths": ["dst"] }
	}));
	assert!(!root.path().join("dst").exists());
}

#[test]
fn move_file_target_exists_errors() {
	let root = tempfile::tempdir().expect("tempdir");
	write_text(&root.path().join("from.txt"), "from");
	write_text(&root.path().join("to.txt"), "to");
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send("tools/call", json!({
		"name": "move_file",
		"arguments": { "from": "from.txt", "to": "to.txt" }
	}));
	let result = resp.get("result").expect("result");
	assert_eq!(result.get("isError").and_then(Value::as_bool), Some(true));
	let code = result.get("structuredContent")
		.and_then(|s| s.get("code"))
		.and_then(Value::as_str);
	assert_eq!(code, Some("TARGET_EXISTS"));
}

#[test]
fn move_file_scope_grant_ping_pong() {
	let base = tempfile::tempdir().expect("tempdir");
	let root_dir = base.path().join("root");
	let external_dir = base.path().join("external");
	std::fs::create_dir_all(&root_dir).expect("root dir");
	std::fs::create_dir_all(&external_dir).expect("external dir");
	write_text(&external_dir.join("from.txt"), "from");
	let mut client = RpcClient::spawn(&root_dir);
	let resp = client.send(
		"tools/call",
		json!({
			"name": "move_file",
			"arguments": { "from": "../external/from.txt", "to": "../external/to.txt" }
		})
	);
	let meta = resp.get("result")
		.and_then(|result| result.get("_meta"))
		.expect("meta");
	let scopes = meta.get("requested_scopes")
		.and_then(Value::as_array)
		.expect("requested_scopes");
	let scope = scopes[0].as_str()
		.expect("scope string")
		.to_string();
	let _ = client.send(
		"tools/call",
		json!({
			"name": "move_file",
			"arguments": { "from": "../external/from.txt", "to": "../external/to.txt" },
			"_meta": { "allowed_scopes": [scope] }
		})
	);
	assert!(!external_dir.join("from.txt").exists());
	assert!(external_dir.join("to.txt").exists());
}

#[test]
fn delete_file_partial_failure_reports_per_path_results() {
	let root = tempfile::tempdir().expect("tempdir");
	write_text(&root.path().join("keep.txt"), "data");
	let mut client = RpcClient::spawn(root.path());
	let resp = client.send("tools/call", json!({
		"name": "delete_file",
		"arguments": { "paths": ["keep.txt", "missing.txt"] }
	}));
	let result = resp.get("result").expect("result");
	assert_eq!(result.get("isError").and_then(Value::as_bool), Some(true));
	let structured = result.get("structuredContent").expect("structuredContent");
	assert_eq!(structured.get("code").and_then(Value::as_str), Some("PARTIAL_DELETE_FAILED"));
	assert_eq!(structured.get("deleted_count").and_then(Value::as_u64), Some(1));
	assert_eq!(structured.get("failed_count").and_then(Value::as_u64), Some(1));
	let results = structured.get("results")
		.and_then(Value::as_array)
		.expect("results");
	assert_eq!(results.len(), 2);
	assert_eq!(results[0].get("path").and_then(Value::as_str), Some("keep.txt"));
	assert_eq!(results[0].get("status").and_then(Value::as_str), Some("deleted"));
	assert_eq!(results[1].get("path").and_then(Value::as_str), Some("missing.txt"));
	assert_eq!(results[1].get("status").and_then(Value::as_str), Some("failed"));
	assert!(!root.path().join("keep.txt").exists());
}

#[test]
fn delete_file_scope_grant_ping_pong() {
	let base = tempfile::tempdir().expect("tempdir");
	let root_dir = base.path().join("root");
	let external_dir = base.path().join("external");
	std::fs::create_dir_all(&root_dir).expect("root dir");
	std::fs::create_dir_all(&external_dir).expect("external dir");
	write_text(&external_dir.join("delete.txt"), "data");
	let mut client = RpcClient::spawn(&root_dir);
	let resp = client.send("tools/call", json!({
		"name": "delete_file",
		"arguments": { "paths": ["../external/delete.txt"] }
	}));
	let result = resp.get("result").expect("result");
	assert_eq!(result.get("isError").and_then(Value::as_bool), Some(true));
	let meta = result.get("_meta").expect("meta");
	let scopes = meta.get("requested_scopes")
		.and_then(Value::as_array)
		.expect("requested_scopes");
	let scope = scopes[0].as_str()
		.expect("scope string")
		.to_string();
	let _ = client.send(
		"tools/call",
		json!({
			"name": "delete_file",
			"arguments": { "paths": ["../external/delete.txt"] },
			"_meta": { "allowed_scopes": [scope] }
		})
	);
	assert!(!external_dir.join("delete.txt").exists());
}
