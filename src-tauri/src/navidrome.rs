//! Navidrome（自建音乐库）接入：Subsonic API v1.16.1 客户端。
//!
//! Navidrome 完整兼容 Subsonic REST API，认证采用 token 方案：
//!   token = md5(password + salt)，每次请求附带 u / t / s / v / c / f 参数。
//! 本模块实现：ping（连通性测试）、search3（搜索）、stream（播放）、
//! getCoverArt（封面）、getLyricsBySongId / getLyrics（歌词）。
use md5::{Digest, Md5};
use serde::Serialize;
use std::time::Duration;

use crate::db;
use crate::models::LyricsPayload;

const API_VERSION: &str = "1.16.1";
const CLIENT: &str = "RustMusic";
const TIMEOUT: Duration = Duration::from_secs(12);

/// 连接配置（存于 settings 表）
pub struct NdConfig {
    pub url: String,
    pub user: String,
    pub pass: String,
}

pub fn load_config(conn: &rusqlite::Connection) -> Option<NdConfig> {
    let url = db::get_setting(conn, "navidrome_url").unwrap_or_default();
    let user = db::get_setting(conn, "navidrome_user").unwrap_or_default();
    let pass = db::get_setting(conn, "navidrome_pass").unwrap_or_default();
    if url.is_empty() || user.is_empty() {
        return None;
    }
    Some(NdConfig {
        url: url.trim_end_matches('/').to_string(),
        user,
        pass,
    })
}

fn random_salt() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"abcdef0123456789";
    let mut rng = rand::thread_rng();
    (0..12)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

/// 公共认证参数串（token = md5(password + salt)）
fn auth_query(cfg: &NdConfig) -> String {
    let salt = random_salt();
    let mut h = Md5::new();
    h.update(format!("{}{}", cfg.pass, salt).as_bytes());
    let token = hex::encode(h.finalize());
    let u = percent_encoding::utf8_percent_encode(&cfg.user, percent_encoding::NON_ALPHANUMERIC);
    format!("u={u}&t={token}&s={salt}&v={API_VERSION}&c={CLIENT}&f=json")
}

fn enc(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn get_json(cfg: &NdConfig, path: &str, extra: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/rest/{}?{}&{}", cfg.url, path, auth_query(cfg), extra);
    let resp = ureq::get(&url)
        .timeout(TIMEOUT)
        .call()
        .map_err(|e| format!("连接 Navidrome 失败：{e}"))?;
    let body: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("响应解析失败：{e}"))?;
    let root = body
        .get("subsonic-response")
        .ok_or("Navidrome 响应格式异常")?;
    if root.get("status").and_then(|s| s.as_str()) != Some("ok") {
        let err = root.get("error");
        let msg = err
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("未知错误");
        let code = err
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_i64())
            .unwrap_or(0);
        return Err(match code {
            40 => "用户名或密码错误".to_string(),
            _ => format!("Navidrome 错误：{msg}"),
        });
    }
    Ok(root.clone())
}

// ---------- 数据结构 ----------

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NdSong {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// 封面完整 URL（getCoverArt）
    pub cover: String,
    pub duration_ms: u64,
    pub suffix: String,
    pub bitrate: i64,
}

// ---------- API ----------

/// 连通性测试，成功返回服务器名称（如 "Navidrome 0.54.0"）
pub fn ping(cfg: &NdConfig) -> Result<String, String> {
    let root = get_json(cfg, "ping", "")?;
    let name = root
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("Navidrome")
        .to_string();
    let ver = root
        .get("serverVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Ok(if ver.is_empty() { name } else { format!("{name} {ver}") })
}

/// search3 搜索歌曲（Navidrome 兼容 Subsonic search3）
pub fn search3(
    cfg: &NdConfig,
    keyword: &str,
    count: i64,
    offset: i64,
) -> Result<Vec<NdSong>, String> {
    let root = get_json(
        cfg,
        "search3",
        &format!(
            "query={}&songCount={count}&songOffset={offset}&artistCount=0&albumCount=0",
            enc(keyword)
        ),
    )?;
    let empty = vec![];
    let songs = root
        .get("searchResult3")
        .and_then(|r| r.get("song"))
        .and_then(|s| s.as_array())
        .unwrap_or(&empty);
    Ok(songs
        .iter()
        .map(|s| {
            let id = s.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let cover_art = s.get("coverArt").and_then(|v| v.as_str()).unwrap_or("");
            let cover = if cover_art.is_empty() {
                String::new()
            } else {
                cover_url(cfg, cover_art, 600)
            };
            NdSong {
                id,
                title: s.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                artist: s.get("artist").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                album: s.get("album").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                cover,
                duration_ms: (s.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0) * 1000.0)
                    as u64,
                suffix: s.get("suffix").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                bitrate: s.get("bitRate").and_then(|v| v.as_i64()).unwrap_or(0),
            }
        })
        .collect())
}

/// 播放链接（stream 端点，带认证参数的原文件流）
pub fn stream_url(cfg: &NdConfig, id: &str) -> String {
    format!("{}/rest/stream?{}&id={}", cfg.url, auth_query(cfg), enc(id))
}

/// 封面 URL（getCoverArt）
pub fn cover_url(cfg: &NdConfig, cover_art_id: &str, size: u32) -> String {
    format!(
        "{}/rest/getCoverArt?{}&id={}&size={}",
        cfg.url,
        auth_query(cfg),
        enc(cover_art_id),
        size
    )
}

/// 歌词：优先 getLyricsBySongId（OpenSubsonic，Navidrome 支持逐字/逐行），
/// 回落 getLyrics（按 artist/title，纯文本）。
pub fn lyric(cfg: &NdConfig, id: &str) -> Result<Option<LyricsPayload>, String> {
    // 1) getLyricsBySongId
    if let Ok(root) = get_json(cfg, "getLyricsBySongId", &format!("id={id}")) {
        if let Some(list) = root.get("lyricsList") {
            if let Some(payload) = parse_lyrics_list(list) {
                return Ok(Some(payload));
            }
        }
    }
    // 2) getLyrics（需要歌曲信息：先 getSong 取 artist/title）
    let song = get_json(cfg, "getSong", &format!("id={id}"))?;
    let (artist, title) = song
        .get("song")
        .map(|s| {
            (
                s.get("artist").and_then(|v| v.as_str()).unwrap_or(""),
                s.get("title").and_then(|v| v.as_str()).unwrap_or(""),
            )
        })
        .unwrap_or(("", ""));
    if artist.is_empty() && title.is_empty() {
        return Ok(None);
    }
    let root = match get_json(
        cfg,
        "getLyrics",
        &format!("artist={}&title={}", enc(artist), enc(title)),
    ) {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };
    let text = root
        .get("lyrics")
        .and_then(|l| l.get("value"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if text.is_empty() {
        return Ok(None);
    }
    let p = crate::lyrics::parse(text);
    Ok(Some(LyricsPayload {
        synced: p.synced,
        lines: p.lines,
    }))
}

/// 将 OpenSubsonic lyricsList（structuredLines / line）转为 LRC 文本后解析
fn parse_lyrics_list(list: &serde_json::Value) -> Option<LyricsPayload> {
    // structuredLines: [{ start: ms?, line: [{ start?, value }] }]
    let structured = list.get("structuredLines").and_then(|v| v.as_array());
    if let Some(lines) = structured {
        let mut out = String::new();
        let mut synced = false;
        for entry in lines {
            let text = entry
                .get("line")
                .and_then(|l| l.as_array())
                .map(|words| {
                    words
                        .iter()
                        .filter_map(|w| w.get("value").and_then(|v| v.as_str()))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            // 时间取行 start，缺省取首字 start
            let start_ms = entry
                .get("start")
                .and_then(|v| v.as_i64())
                .or_else(|| {
                    entry
                        .get("line")
                        .and_then(|l| l.as_array())
                        .and_then(|w| w.first())
                        .and_then(|w| w.get("start"))
                        .and_then(|v| v.as_i64())
                });
            push_line(&mut out, &mut synced, start_ms, &text);
        }
        if !out.trim().is_empty() {
            let p = crate::lyrics::parse(&out);
            return Some(LyricsPayload {
                synced: synced && p.synced,
                lines: p.lines,
            });
        }
        return None;
    }
    // line: [{ start?, value }]
    let plain = list.get("line").and_then(|v| v.as_array());
    if let Some(lines) = plain {
        let mut out = String::new();
        let mut synced = false;
        for entry in lines {
            let text = entry
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let start_ms = entry.get("start").and_then(|v| v.as_i64());
            push_line(&mut out, &mut synced, start_ms, &text);
        }
        if !out.trim().is_empty() {
            let p = crate::lyrics::parse(&out);
            return Some(LyricsPayload {
                synced: synced && p.synced,
                lines: p.lines,
            });
        }
    }
    None
}

fn push_line(out: &mut String, synced: &mut bool, start_ms: Option<i64>, text: &str) {
    if let Some(ms) = start_ms {
        *synced = true;
        let m = ms / 60000;
        let s = (ms % 60000) / 1000;
        let cs = (ms % 1000) / 10;
        out.push_str(&format!("[{m:02}:{s:02}.{cs:02}]{text}\n"));
    } else {
        out.push_str(&format!("{text}\n"));
    }
}
