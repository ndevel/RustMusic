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
        .map(|s| parse_nd_song(cfg, s))
        .collect())
}

/// getRandomSongs：随机获取歌曲列表（登录后默认展示用）
pub fn get_random_songs(cfg: &NdConfig, count: i64) -> Result<Vec<NdSong>, String> {
    let root = get_json(cfg, "getRandomSongs", &format!("size={count}"))?;
    let empty = vec![];
    let songs = root
        .get("randomSongs")
        .and_then(|r| r.get("song"))
        .and_then(|s| s.as_array())
        .unwrap_or(&empty);
    Ok(songs
        .iter()
        .map(|s| parse_nd_song(cfg, s))
        .collect())
}

fn parse_nd_song(cfg: &NdConfig, s: &serde_json::Value) -> NdSong {
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

/// OpenSubsonic: lyricsList.structuredLyrics[].line[]。
/// 每项是一份完整歌词（可能为不同语言），优先同步版本，不拼接多个版本。
fn parse_lyrics_list(list: &serde_json::Value) -> Option<LyricsPayload> {
    let versions = list.get("structuredLyrics")?.as_array()?;
    let mut best: Option<LyricsPayload> = None;
    for version in versions {
        let Some(entries) = version.get("line").and_then(|v| v.as_array()) else {
            continue;
        };
        let synced = version.get("synced").and_then(|v| v.as_bool()).unwrap_or(false);
        // OpenSubsonic 正偏移表示提前显示，与内部 LRC offset 的方向不同。
        let offset = version.get("offset").and_then(|v| v.as_i64()).unwrap_or(0);
        let mut lines = Vec::new();
        for entry in entries {
            let Some(text) = entry.get("value").and_then(|v| v.as_str()) else {
                continue;
            };
            let time_ms = if synced {
                entry.get("start").and_then(|v| v.as_i64())
                    .map(|ms| ms.saturating_sub(offset).max(0) as u64)
            } else {
                None
            };
            lines.push(crate::models::LyricLine {
                time_ms,
                text: text.to_string(),
                words: None,
            });
        }
        if !lines.iter().any(|line| !line.text.trim().is_empty()) {
            continue;
        }
        let synced = synced && lines.iter().any(|line| line.time_ms.is_some());
        if synced {
            lines.sort_by_key(|line| line.time_ms.unwrap_or(u64::MAX));
        }
        let payload = LyricsPayload { synced, lines };
        if best.as_ref().map_or(true, |old| payload.synced && !old.synced) {
            best = Some(payload);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn embedded_plain_lyrics_from_standard_response() {
        let list = json!({"structuredLyrics": [{"lang":"xxx", "synced":false,
            "line":[{"value":"第一句"},{"value":"第二句"}]}]});
        let p = parse_lyrics_list(&list).unwrap();
        assert!(!p.synced);
        assert_eq!(p.lines.len(), 2);
        assert_eq!(p.lines[0].text, "第一句");
        assert_eq!(p.lines[0].time_ms, None);
    }

    #[test]
    fn prefers_synced_and_applies_offset_without_losing_precision() {
        let list = json!({"structuredLyrics": [
            {"synced":false,"line":[{"value":"普通文本"}]},
            {"synced":true,"offset":100,"line":[
                {"start":3001,"value":"第二句"},{"start":0,"value":"第一句"}]}]});
        let p = parse_lyrics_list(&list).unwrap();
        assert!(p.synced);
        assert_eq!(p.lines[0].time_ms, Some(0));
        assert_eq!(p.lines[1].time_ms, Some(2901));
        let serialized = serde_json::to_value(p).unwrap();
        assert_eq!(serialized["lines"][1]["timeMs"], 2901);
    }

    #[test]
    fn negative_offset_delays_and_empty_versions_are_skipped() {
        let list = json!({"structuredLyrics": [{"line":[]},
            {"synced":true,"offset":-100,"line":[{"start":0,"value":"歌词"}]}]});
        assert_eq!(parse_lyrics_list(&list).unwrap().lines[0].time_ms, Some(100));
        assert!(parse_lyrics_list(&json!({"structuredLyrics":[]})).is_none());
        assert!(parse_lyrics_list(&json!({})).is_none());
    }
}
