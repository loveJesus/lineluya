// For God so loved the world that he gave his only begotten Son,
// that whoever believes in him should not perish but have eternal life. - John 3:16

//! Lineluya Origin Server — Rust WebSocket filesystem relay.
//!
//! This server runs on a VPS/dedicated host with a real Linux filesystem.
//! The Cloudflare Worker connects to it via WebSocket and relays filesystem
//! operations from the edge WASM kernel. This makes the server's filesystem
//! appear as if it's connected to the browser kernel.
//!
//! Protocol (CF Worker <-> Origin Server):
//!   { "op_chirho": "read_chirho", "path_chirho": "/etc/passwd", "offset_chirho": 0, "len_chirho": 4096 }
//!   { "op_chirho": "write_chirho", "path_chirho": "/tmp/foo", "data_chirho": "base64..." }
//!   { "op_chirho": "stat_chirho", "path_chirho": "/bin/ls" }
//!   { "op_chirho": "readdir_chirho", "path_chirho": "/etc" }
//!   { "op_chirho": "exec_chirho", "cmd_chirho": "ls", "args_chirho": ["-la"], "cwd_chirho": "/" }
//!
//! Run with: cargo run --manifest-path web-chirho/origin-server-chirho/Cargo.toml

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_CHIRHO;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

/// Root directory that the origin server exposes.
/// Everything outside this path is denied for security.
const ROOTFS_PATH_CHIRHO: &str = "/tmp/lineluya-rootfs-chirho";

/// Default listen port.
const DEFAULT_PORT_CHIRHO: u16 = 9876;

/// Inbound command from CF Worker.
#[derive(Debug, Deserialize)]
struct CommandChirho {
    op_chirho: String,
    path_chirho: Option<String>,
    offset_chirho: Option<u64>,
    len_chirho: Option<u64>,
    data_chirho: Option<String>,
    cmd_chirho: Option<String>,
    args_chirho: Option<Vec<String>>,
    cwd_chirho: Option<String>,
    id_chirho: Option<String>,
}

/// Response sent back to CF Worker.
#[derive(Debug, Serialize)]
struct ResponseChirho {
    op_chirho: String,
    id_chirho: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_chirho: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_chirho: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entries_chirho: Option<Vec<DirEntryChirho>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stat_chirho: Option<StatInfoChirho>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code_chirho: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stdout_chirho: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stderr_chirho: Option<String>,
}

#[derive(Debug, Serialize)]
struct DirEntryChirho {
    name_chirho: String,
    is_dir_chirho: bool,
    size_chirho: u64,
}

#[derive(Debug, Serialize)]
struct StatInfoChirho {
    size_chirho: u64,
    is_dir_chirho: bool,
    is_file_chirho: bool,
    is_symlink_chirho: bool,
    mode_chirho: u32,
    modified_chirho: Option<u64>,
}

/// Resolve a virtual path to a real path within the rootfs jail.
fn resolve_path_chirho(virtual_path_chirho: &str) -> Result<PathBuf, String> {
    let clean_chirho = virtual_path_chirho.replace("..", "");
    let real_chirho = PathBuf::from(ROOTFS_PATH_CHIRHO).join(
        clean_chirho.trim_start_matches('/'),
    );
    // Ensure the resolved path is within our rootfs
    let canonical_root_chirho = std::fs::canonicalize(ROOTFS_PATH_CHIRHO)
        .unwrap_or_else(|_| PathBuf::from(ROOTFS_PATH_CHIRHO));
    let canonical_path_chirho = real_chirho.clone();
    if !canonical_path_chirho.starts_with(&canonical_root_chirho)
        && !real_chirho.starts_with(ROOTFS_PATH_CHIRHO)
    {
        return Err("Path escapes rootfs jail".to_string());
    }
    Ok(real_chirho)
}

/// Handle a single filesystem command.
async fn handle_command_chirho(cmd_chirho: CommandChirho) -> ResponseChirho {
    let id_chirho = cmd_chirho.id_chirho.clone();
    let op_chirho = cmd_chirho.op_chirho.clone();

    match cmd_chirho.op_chirho.as_str() {
        "read_chirho" => {
            let path_chirho = match cmd_chirho.path_chirho.as_deref() {
                Some(p_chirho) => p_chirho,
                None => return error_response_chirho(&op_chirho, id_chirho, "Missing path_chirho"),
            };
            match resolve_path_chirho(path_chirho) {
                Ok(real_path_chirho) => {
                    match tokio::fs::read(&real_path_chirho).await {
                        Ok(data_chirho) => {
                            let offset_chirho = cmd_chirho.offset_chirho.unwrap_or(0) as usize;
                            let len_chirho = cmd_chirho.len_chirho.unwrap_or(data_chirho.len() as u64) as usize;
                            let end_chirho = std::cmp::min(offset_chirho + len_chirho, data_chirho.len());
                            let slice_chirho = if offset_chirho < data_chirho.len() {
                                &data_chirho[offset_chirho..end_chirho]
                            } else {
                                &[]
                            };
                            ResponseChirho {
                                op_chirho: "read_response_chirho".to_string(),
                                id_chirho,
                                data_chirho: Some(BASE64_CHIRHO.encode(slice_chirho)),
                                error_chirho: None,
                                entries_chirho: None,
                                stat_chirho: None,
                                exit_code_chirho: None,
                                stdout_chirho: None,
                                stderr_chirho: None,
                            }
                        }
                        Err(e_chirho) => error_response_chirho(&op_chirho, id_chirho, &e_chirho.to_string()),
                    }
                }
                Err(e_chirho) => error_response_chirho(&op_chirho, id_chirho, &e_chirho),
            }
        }

        "write_chirho" => {
            let path_chirho = match cmd_chirho.path_chirho.as_deref() {
                Some(p_chirho) => p_chirho,
                None => return error_response_chirho(&op_chirho, id_chirho, "Missing path_chirho"),
            };
            let data_b64_chirho = match cmd_chirho.data_chirho.as_deref() {
                Some(d_chirho) => d_chirho,
                None => return error_response_chirho(&op_chirho, id_chirho, "Missing data_chirho"),
            };
            match resolve_path_chirho(path_chirho) {
                Ok(real_path_chirho) => {
                    match BASE64_CHIRHO.decode(data_b64_chirho) {
                        Ok(bytes_chirho) => {
                            // Ensure parent directory exists
                            if let Some(parent_chirho) = real_path_chirho.parent() {
                                let _ = tokio::fs::create_dir_all(parent_chirho).await;
                            }
                            match tokio::fs::write(&real_path_chirho, &bytes_chirho).await {
                                Ok(()) => ResponseChirho {
                                    op_chirho: "write_response_chirho".to_string(),
                                    id_chirho,
                                    data_chirho: None,
                                    error_chirho: None,
                                    entries_chirho: None,
                                    stat_chirho: None,
                                    exit_code_chirho: None,
                                    stdout_chirho: None,
                                    stderr_chirho: None,
                                },
                                Err(e_chirho) => error_response_chirho(&op_chirho, id_chirho, &e_chirho.to_string()),
                            }
                        }
                        Err(e_chirho) => error_response_chirho(&op_chirho, id_chirho, &e_chirho.to_string()),
                    }
                }
                Err(e_chirho) => error_response_chirho(&op_chirho, id_chirho, &e_chirho),
            }
        }

        "stat_chirho" => {
            let path_chirho = match cmd_chirho.path_chirho.as_deref() {
                Some(p_chirho) => p_chirho,
                None => return error_response_chirho(&op_chirho, id_chirho, "Missing path_chirho"),
            };
            match resolve_path_chirho(path_chirho) {
                Ok(real_path_chirho) => {
                    match tokio::fs::metadata(&real_path_chirho).await {
                        Ok(meta_chirho) => {
                            let modified_chirho = meta_chirho.modified().ok().and_then(|t_chirho| {
                                t_chirho.duration_since(std::time::UNIX_EPOCH).ok().map(|d_chirho| d_chirho.as_secs())
                            });
                            ResponseChirho {
                                op_chirho: "stat_response_chirho".to_string(),
                                id_chirho,
                                data_chirho: None,
                                error_chirho: None,
                                entries_chirho: None,
                                stat_chirho: Some(StatInfoChirho {
                                    size_chirho: meta_chirho.len(),
                                    is_dir_chirho: meta_chirho.is_dir(),
                                    is_file_chirho: meta_chirho.is_file(),
                                    is_symlink_chirho: meta_chirho.is_symlink(),
                                    mode_chirho: 0o644,
                                    modified_chirho,
                                }),
                                exit_code_chirho: None,
                                stdout_chirho: None,
                                stderr_chirho: None,
                            }
                        }
                        Err(e_chirho) => error_response_chirho(&op_chirho, id_chirho, &e_chirho.to_string()),
                    }
                }
                Err(e_chirho) => error_response_chirho(&op_chirho, id_chirho, &e_chirho),
            }
        }

        "readdir_chirho" => {
            let path_chirho = match cmd_chirho.path_chirho.as_deref() {
                Some(p_chirho) => p_chirho,
                None => return error_response_chirho(&op_chirho, id_chirho, "Missing path_chirho"),
            };
            match resolve_path_chirho(path_chirho) {
                Ok(real_path_chirho) => {
                    match tokio::fs::read_dir(&real_path_chirho).await {
                        Ok(mut dir_chirho) => {
                            let mut entries_chirho = Vec::new();
                            while let Ok(Some(entry_chirho)) = dir_chirho.next_entry().await {
                                let meta_chirho = entry_chirho.metadata().await;
                                entries_chirho.push(DirEntryChirho {
                                    name_chirho: entry_chirho.file_name().to_string_lossy().to_string(),
                                    is_dir_chirho: meta_chirho.as_ref().map(|m_chirho| m_chirho.is_dir()).unwrap_or(false),
                                    size_chirho: meta_chirho.as_ref().map(|m_chirho| m_chirho.len()).unwrap_or(0),
                                });
                            }
                            ResponseChirho {
                                op_chirho: "readdir_response_chirho".to_string(),
                                id_chirho,
                                data_chirho: None,
                                error_chirho: None,
                                entries_chirho: Some(entries_chirho),
                                stat_chirho: None,
                                exit_code_chirho: None,
                                stdout_chirho: None,
                                stderr_chirho: None,
                            }
                        }
                        Err(e_chirho) => error_response_chirho(&op_chirho, id_chirho, &e_chirho.to_string()),
                    }
                }
                Err(e_chirho) => error_response_chirho(&op_chirho, id_chirho, &e_chirho),
            }
        }

        "exec_chirho" => {
            let cmd_str_chirho = match cmd_chirho.cmd_chirho.as_deref() {
                Some(c_chirho) => c_chirho,
                None => return error_response_chirho(&op_chirho, id_chirho, "Missing cmd_chirho"),
            };
            let args_chirho = cmd_chirho.args_chirho.unwrap_or_default();
            let cwd_chirho = cmd_chirho.cwd_chirho.unwrap_or_else(|| "/".to_string());

            let cwd_resolved_chirho = resolve_path_chirho(&cwd_chirho).unwrap_or_else(|_| PathBuf::from(ROOTFS_PATH_CHIRHO));

            match tokio::process::Command::new(cmd_str_chirho)
                .args(&args_chirho)
                .current_dir(&cwd_resolved_chirho)
                .output()
                .await
            {
                Ok(output_chirho) => ResponseChirho {
                    op_chirho: "exec_response_chirho".to_string(),
                    id_chirho,
                    data_chirho: None,
                    error_chirho: None,
                    entries_chirho: None,
                    stat_chirho: None,
                    exit_code_chirho: output_chirho.status.code(),
                    stdout_chirho: Some(BASE64_CHIRHO.encode(&output_chirho.stdout)),
                    stderr_chirho: Some(BASE64_CHIRHO.encode(&output_chirho.stderr)),
                },
                Err(e_chirho) => error_response_chirho(&op_chirho, id_chirho, &e_chirho.to_string()),
            }
        }

        _ => error_response_chirho(&op_chirho, id_chirho, "Unknown operation"),
    }
}

fn error_response_chirho(
    op_chirho: &str,
    id_chirho: Option<String>,
    msg_chirho: &str,
) -> ResponseChirho {
    ResponseChirho {
        op_chirho: format!("{}_error_chirho", op_chirho),
        id_chirho,
        data_chirho: None,
        error_chirho: Some(msg_chirho.to_string()),
        entries_chirho: None,
        stat_chirho: None,
        exit_code_chirho: None,
        stdout_chirho: None,
        stderr_chirho: None,
    }
}

#[tokio::main]
async fn main() {
    let port_chirho = std::env::var("ORIGIN_PORT_CHIRHO")
        .ok()
        .and_then(|p_chirho| p_chirho.parse().ok())
        .unwrap_or(DEFAULT_PORT_CHIRHO);

    let addr_chirho = SocketAddr::from(([0, 0, 0, 0], port_chirho));

    // Ensure rootfs directory exists
    std::fs::create_dir_all(ROOTFS_PATH_CHIRHO).ok();

    let listener_chirho = TcpListener::bind(&addr_chirho)
        .await
        .expect("Failed to bind");

    println!("[ORIGIN] Lineluya Origin Server listening on ws://{}", addr_chirho);
    println!("[ORIGIN] Rootfs: {}", ROOTFS_PATH_CHIRHO);
    println!("[ORIGIN] For God so loved the world - John 3:16");

    while let Ok((stream_chirho, peer_chirho)) = listener_chirho.accept().await {
        tokio::spawn(async move {
            println!("[ORIGIN] Connection from {}", peer_chirho);

            let ws_chirho = match accept_async(stream_chirho).await {
                Ok(ws_chirho) => ws_chirho,
                Err(e_chirho) => {
                    eprintln!("[ORIGIN] WebSocket handshake failed: {}", e_chirho);
                    return;
                }
            };

            let (mut write_chirho, mut read_chirho) = ws_chirho.split();

            while let Some(msg_result_chirho) = read_chirho.next().await {
                match msg_result_chirho {
                    Ok(Message::Text(text_chirho)) => {
                        match serde_json::from_str::<CommandChirho>(&text_chirho) {
                            Ok(cmd_chirho) => {
                                let response_chirho = handle_command_chirho(cmd_chirho).await;
                                let json_chirho = serde_json::to_string(&response_chirho)
                                    .unwrap_or_else(|_| "{}".to_string());
                                if write_chirho.send(Message::Text(json_chirho.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(e_chirho) => {
                                let err_response_chirho = error_response_chirho(
                                    "parse",
                                    None,
                                    &format!("Invalid JSON: {}", e_chirho),
                                );
                                let json_chirho = serde_json::to_string(&err_response_chirho)
                                    .unwrap_or_else(|_| "{}".to_string());
                                if write_chirho.send(Message::Text(json_chirho.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(_) => break,
                    _ => {}
                }
            }

            println!("[ORIGIN] Connection from {} closed", peer_chirho);
        });
    }
}
