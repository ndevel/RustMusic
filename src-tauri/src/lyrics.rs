use crate::models::{LyricLine, LyricsPayload, Word};
use lofty::prelude::*;
use lofty::tag::{ItemKey, ItemValue};
use std::borrow::Cow;
use std::path::Path;

pub struct Parsed {
    pub synced: bool,
    pub lines: Vec<LyricLine>,
}

/// 解析 LRC 文本：支持多时间标签、[offset:...] 元数据、增强 LRC 的
/// 逐字标签 <mm:ss.xx>（yrc 转换后同样落成该格式）
pub fn parse(text: &str) -> Parsed {
    let mut out: Vec<LyricLine> = Vec::new();
    let mut offset_ms: i64 = 0;

    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times: Vec<u64> = Vec::new();

        loop {
            if !rest.starts_with('[') {
                break;
            }
            let Some(close) = rest.find(']') else { break };
            let tag = &rest[1..close];
            if let Some(t) = parse_time_tag(tag) {
                times.push(t);
                rest = rest[close + 1..].trim_start();
                continue;
            }
            if let Some(v) = tag.strip_prefix("offset:") {
                offset_ms = v.trim().parse::<i64>().unwrap_or(0);
                rest = rest[close + 1..].trim_start();
                continue;
            }
            // 其他元数据标签（[ti:] [ar:] 等）：跳过标签本身
            rest = rest[close + 1..].trim_start();
            break;
        }

        let (content, words) = parse_word_tags(rest);
        let content = content.trim().to_string();
        if times.is_empty() {
            // 无时间标签的内容行（纯文本歌词），元数据行内容通常带冒号被保留——仍展示无妨
            if !content.is_empty() {
                out.push(LyricLine { time_ms: None, text: content, words: None });
            }
        } else {
            let adj = |t: u64| -> u64 {
                let v = t as i64 + offset_ms;
                v.max(0) as u64
            };
            let words = words.map(|ws| {
                ws.into_iter()
                    .map(|w| Word { start_ms: adj(w.0), end_ms: adj(w.1), text: w.2 })
                    .collect::<Vec<_>>()
            });
            for t in times {
                out.push(LyricLine {
                    time_ms: Some(adj(t)),
                    text: content.clone(),
                    words: words.clone(),
                });
            }
        }
    }

    out.sort_by_key(|l| l.time_ms.unwrap_or(u64::MAX));
    let synced = out.iter().any(|l| l.time_ms.is_some());
    Parsed { synced, lines: out }
}

/// 解析增强 LRC 的逐字标签：形如 [00:01.00]<00:01.00>你<00:01.50>好
/// 返回 (去标签文本, 逐字 (start, end, text) 列表)；无逐字标签时 words = None。
/// end 时间 = 下一字的 start；末尾字暂用 start+600ms，调用方按行时长兜底修正。
fn parse_word_tags(s: &str) -> (String, Option<Vec<(u64, u64, String)>>) {
    if !s.contains('<') {
        return (s.to_string(), None);
    }
    let mut text = String::with_capacity(s.len());
    let mut words: Vec<(u64, u64, String)> = Vec::new();
    let mut i = 0usize;
    while i < s.len() {
        if s[i..].starts_with('<') {
            if let Some((t, consumed)) = parse_time_prefix(&s[i + 1..]) {
                let j = i + 1 + consumed; // '>' 之后
                // 字文本：到下一个 '<' 为止
                let k = s[j..].find('<').map(|p| j + p).unwrap_or(s.len());
                let word = &s[j..k];
                text.push_str(word);
                words.push((t, t, word.to_string()));
                i = k;
                continue;
            }
        }
        // 非标签处的普通字符；孤立成对的 <...>（非时间）整段剥除
        if s[i..].starts_with('<') {
            if let Some(gt) = s[i..].find('>') {
                i += gt + 1;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        text.push(ch);
        i += ch.len_utf8();
    }
    if words.is_empty() {
        return (text, None);
    }
    // 回填 end：下一字起始；末尾字给 start+600ms 占位
    for w in 0..words.len() {
        if w + 1 < words.len() {
            words[w].1 = words[w + 1].0;
        } else {
            words[w].1 = words[w].0 + 600;
        }
    }
    (text, Some(words))
}

/// "mm:ss.xx" 开头解析为毫秒；返回 (毫秒, 消费到 '>' 后的字符数)
fn parse_time_prefix(s: &str) -> Option<(u64, usize)> {
    let close = s.find('>')?;
    let t = parse_time_tag(&s[..close])?;
    // 消费：close+1 个字符（含 '>')
    Some((t, close + 1))
}

// ---------- yrc / QRC 逐字歌词 → 增强 LRC ----------

/// yrc（网易云）/ QRC（QQ）逐字歌词格式：
/// 歌词行：[行起始ms,行持续ms]字(字起始ms,字持续ms,0)字(...)…字
/// —— 时间元组**跟在它所属字的后面**；但实测网易云行末常带“尾巴字”：
/// 最后一个字之后没有自己的元组（它的区间由下一行起始时间界定）。
/// 真实样例：
///   [1,4890](1,270,0)词(270,270,0)版(540,270,0)…公(4600,290,0)司
/// “司”即尾巴字——不带元组。旧解析按“元组前文本”配对，尾巴被静默丢弃，
/// 表现为行末最后一个字丢失（播放页/桌面歌词都少字）。
/// 修法：尾巴字的区间 = [前一元组 end, 行 start+行 dur]。
/// 元数据行（元信息 JSON：{"t":ms,"c":[...]} 或纯文本）跳过。
/// 转换为增强 LRC：[mm:ss.cc]<mm:ss.cc>字<mm:ss.cc>...
/// 无法解析出任何歌词行时返回 None（调用方回落行级 LRC）。
pub fn yrc_to_enhanced_lrc(yrc: &str) -> Option<String> {
    let mut out = String::new();
    for line in yrc.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('{') {
            // JSON 元数据行（v1 端点的 {"t":..,"c":[..]} 格式）不进歌词
            continue;
        }
        let rest = line.strip_prefix('[')?;
        let close = rest.find(']')?;
        let header: Vec<&str> = rest[..close].split(',').collect();
        if header.len() < 2 {
            return None;
        }
        let line_start: u64 = header[0].parse().ok()?;
        let line_dur: u64 = header[1].parse().ok()?;
        let line_end = line_start + line_dur;
        let mut body = String::new();
        let words = &rest[close + 1..];
        // 游标扫描：每个 (...) 元组配它前面紧邻的文本段；
        // 元组前无文本（起拍占位）则跳过该元组。
        let mut cursor = 0usize;
        let mut prev_end = line_start;
        while let Some(rel) = words[cursor..].find('(') {
            let lp = cursor + rel;
            let text = words[cursor..lp].trim_start();
            let rp = words[lp..].find(')')? + lp;
            // 字元组 (start,dur[,ext...])：只取前两个，容忍第三参数
            let times: Vec<&str> = words[lp + 1..rp].split(',').collect();
            if times.len() < 2 {
                return None;
            }
            let ws: u64 = times[0].parse().ok()?;
            let wd: u64 = times[1].parse().ok()?;
            if !text.is_empty() {
                let cs = fmt_lrc_time(ws);
                let ce = fmt_lrc_time(ws + wd);
                body.push_str(&format!("<{cs}>{text}<{ce}>"));
            }
            prev_end = prev_end.max(ws + wd);
            cursor = rp + 1;
        }
        // 行末尾巴字：最后一个元组之后仍残留的文本（它没有自己的元组），
        // 区间 = [前一元组 end, 行 start+行 dur]
        let tail = words[cursor..].trim_start();
        if !tail.is_empty() && !body.is_empty() {
            let cs = fmt_lrc_time(prev_end);
            let ce = fmt_lrc_time(line_end.max(prev_end));
            body.push_str(&format!("<{cs}>{tail}<{ce}>"));
        }
        if body.is_empty() {
            continue;
        }
        let ls = fmt_lrc_time(line_start);
        out.push_str(&format!("[{ls}]{body}\n"));
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// 把解析后的歌词还原为 LRC 文本（下载写标签用）。
/// 同步行带 [mm:ss.cc] 时间标签，非同步行按纯文本输出。
pub fn to_lrc(payload: &LyricsPayload) -> String {
    let mut out = String::new();
    for line in &payload.lines {
        if line.text.trim().is_empty() {
            continue;
        }
        match line.time_ms {
            Some(ms) => out.push_str(&format!("[{}]{}\n", fmt_lrc_time(ms), line.text)),
            None => out.push_str(&format!("{}\n", line.text)),
        }
    }
    out
}

/// 毫秒 → LRC 时间 mm:ss.cc（百分秒，yrc/QRC 原始精度 10ms）
pub fn fmt_lrc_time(ms: u64) -> String {
    let csec = ms / 10;
    format!("{}:{:02}.{:02}", csec / 6000, (csec / 100) % 60, csec % 100)
}

// ---------- 内嵌歌词（音频标签） ----------

/// 从音频文件标签中提取内嵌歌词。覆盖常见存放方式：
/// - 通用标签键：ID3v2 USLT、Vorbis `LYRICS`、MP4 `©lyr`、APE `Lyrics`
/// - 自定义键：名称含 "lyric" 的文本项（如 TXXX 描述、Vorbis 自定义键）
/// - MP3 深度读取：TXXX:Lyrics 与 SYLT 同步歌词（通用转换会丢弃这两种帧）
/// 优先返回同步歌词；仅当完全没有时间标签时 synced = false。
pub fn embedded(path: &Path) -> Option<LyricsPayload> {
    let mut best: Option<Parsed> = None;

    /// 收集候选文本：优先保留同步歌词（synced），同级别取先到者
    fn consider_text(best: &mut Option<Parsed>, text: &str) {
        let p = parse(text);
        if p.lines.is_empty() {
            return;
        }
        let better = match &best {
            None => true,
            Some(b) => p.synced && !b.synced,
        };
        if better {
            *best = Some(p);
        }
    }

    // 1) 通用标签读取（覆盖 FLAC/OGG/M4A/APE 与 MP3 的 USLT）
    if let Ok(tagged) = lofty::read_from_path(path) {
        for tag in tagged.tags() {
            if let Some(text) = tag.get_string(&ItemKey::Lyrics) {
                consider_text(&mut best, text);
            }
            // 自定义键（大小写/前缀各异，如 "LYRICS"、"Unsynchronized lyrics"）
            for item in tag.items() {
                let key = match item.key() {
                    ItemKey::Unknown(k) => k,
                    _ => continue,
                };
                if !key.to_ascii_lowercase().contains("lyric") {
                    continue;
                }
                if let ItemValue::Text(s) = item.value() {
                    consider_text(&mut best, s);
                }
            }
        }
    }

    // 2) MP3 深度读取：TXXX 与 SYLT 帧在通用 Tag 转换中会被丢弃
    if let Ok(mut fp) = std::fs::File::open(path) {
        if let Ok(file) =
            lofty::mpeg::MpegFile::read_from(&mut fp, lofty::config::ParseOptions::new())
        {
            if let Some(id3) = file.id3v2() {
                // 所有 USLT 帧（get_string 只取第一条，这里全量收集）
                for uslt in id3.unsync_text() {
                    consider_text(&mut best, &uslt.content);
                }
                // TXXX 常见描述名
                for desc in ["Lyrics", "LYRICS", "lyrics", "UnsyncedLyrics", "SyncedLyrics"] {
                    if let Some(t) = id3.get_user_text(desc) {
                        consider_text(&mut best, t);
                    }
                }
                // SYLT 同步歌词（lofty 读作 Binary 帧，手动解析为 LRC）
                if let Some(fr) =
                    id3.get(&lofty::id3::v2::FrameId::Valid(Cow::Borrowed("SYLT")))
                {
                    if let lofty::id3::v2::Frame::Binary(bin) = fr {
                        if let Some(lrc) = parse_sylt(&bin.data) {
                            consider_text(&mut best, &lrc);
                        }
                    }
                }
            }
        }
    }

    best.map(|p| LyricsPayload { synced: p.synced, lines: p.lines })
}

/// 解析 ID3v2 SYLT 帧负载为 LRC 文本。
/// 布局：[encoding:1][language:3][timestamp_format:1][content_type:1]
///       [描述符:按编码终止] + 若干 (文本:按编码终止, 时间戳:u32 BE)
/// 仅支持毫秒时间戳（timestamp_format = 2）。
fn parse_sylt(data: &[u8]) -> Option<String> {
    if data.len() < 7 {
        return None;
    }
    let encoding = data[0];
    if data[4] != 2 {
        // MPEG 帧数时间戳：无法换算为时间轴，跳过
        return None;
    }
    let mut i = 6usize; // 跳过 encoding/language/timestamp_format/content_type
    let mut le: Option<bool> = None;
    // 描述符（跳过）
    let _ = decode_terminated(data, &mut i, encoding, &mut le)?;
    let mut out = String::new();
    while i < data.len() {
        let text = match decode_terminated(data, &mut i, encoding, &mut le) {
            Some(t) => t,
            None => break,
        };
        if i + 4 > data.len() {
            break;
        }
        let ms = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        i += 4;
        if !text.trim().is_empty() {
            let m = ms / 60000;
            let s = (ms % 60000) / 1000;
            let cs = (ms % 1000) / 10;
            out.push_str(&format!("[{m:02}:{s:02}.{cs:02}]{text}\n"));
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// 读取以编码对应终止符结尾的字符串，返回解码文本并推进游标。
/// `le` 记录 UTF-16 的字节序（首个带 BOM 的字符串确定后沿用）。
fn decode_terminated(
    data: &[u8],
    i: &mut usize,
    encoding: u8,
    le: &mut Option<bool>,
) -> Option<String> {
    match encoding {
        0 | 3 => {
            // Latin-1 / UTF-8：单字节 0x00 终止
            let start = *i;
            let end = data[start..].iter().position(|&b| b == 0)? + start;
            *i = end + 1;
            let text = if encoding == 3 {
                String::from_utf8_lossy(&data[start..end]).into_owned()
            } else {
                data[start..end].iter().map(|&b| b as char).collect()
            };
            Some(text)
        }
        1 | 2 => {
            // UTF-16（1 = 带 BOM，2 = 大端）：0x00 0x00 终止，按 2 字节对齐扫描
            let mut start = *i;
            let mut little = match *le {
                Some(v) => v,
                None => {
                    let has_bom = start + 1 < data.len()
                        && ((data[start] == 0xFF && data[start + 1] == 0xFE)
                            || (data[start] == 0xFE && data[start + 1] == 0xFF));
                    let v = if has_bom { data[start] == 0xFF } else { encoding == 1 };
                    *le = Some(v);
                    v
                }
            };
            // 每段开头可能重复出现 BOM：剥除
            if start + 1 < data.len()
                && ((data[start] == 0xFF && data[start + 1] == 0xFE)
                    || (data[start] == 0xFE && data[start + 1] == 0xFF))
            {
                little = data[start] == 0xFF;
                *le = Some(little);
                start += 2;
            }
            let mut units: Vec<u16> = Vec::new();
            let mut j = start;
            while j + 1 < data.len() {
                let u = if little {
                    u16::from_le_bytes([data[j], data[j + 1]])
                } else {
                    u16::from_be_bytes([data[j], data[j + 1]])
                };
                j += 2;
                if u == 0 {
                    break;
                }
                units.push(u);
            }
            *i = j;
            let text = String::from_utf16_lossy(&units);
            // 去掉可能残留的 BOM 字符
            Some(text.trim_start_matches('\u{feff}').to_string())
        }
        _ => None,
    }
}

/// [mm:ss] / [mm:ss.xx] / [mm:ss.xxx] → 毫秒
fn parse_time_tag(tag: &str) -> Option<u64> {
    let (mm, rest) = tag.split_once(':')?;
    let minutes: i64 = mm.trim().parse().ok()?;
    if minutes < 0 {
        return None;
    }
    let secs: f64 = rest.trim().replace(',', ".").parse().ok()?;
    if !(0.0..60.0).contains(&secs) {
        return None;
    }
    Some((minutes as f64 * 60_000.0 + secs * 1000.0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enhanced_lrc_word_tags() {
        let line = "[00:01.00]<00:01.00>你<00:01.50>好<00:02.00>呀";
        let p = parse(line);
        assert_eq!(p.lines.len(), 1);
        let l = &p.lines[0];
        assert_eq!(l.text, "你好呀");
        let ws = l.words.as_ref().expect("words");
        assert_eq!(ws.len(), 3);
        assert_eq!((ws[0].start_ms, ws[0].end_ms, ws[0].text.as_str()), (1000, 1500, "你"));
        assert_eq!((ws[1].start_ms, ws[1].end_ms), (1500, 2000));
        assert_eq!(ws[2].end_ms, 2600); // 末尾字 start+600 兜底
    }

    #[test]
    fn plain_lrc_no_words() {
        let p = parse("[00:10.00]普通歌词");
        assert_eq!(p.lines[0].text, "普通歌词");
        assert!(p.lines[0].words.is_none());
    }

    #[test]
    fn offset_applies_to_words() {
        let p = parse("[offset:500]\n[00:01.00]<00:01.00>你");
        let ws = p.lines[0].words.as_ref().unwrap();
        assert_eq!(ws[0].start_ms, 1500);
    }

    #[test]
    fn to_lrc_roundtrip_keeps_timing_and_plain_lines() {
        let payload = LyricsPayload {
            synced: true,
            lines: vec![
                LyricLine { time_ms: Some(61_234), text: "第一句".into(), words: None },
                LyricLine { time_ms: None, text: "纯文本行".into(), words: None },
                LyricLine { time_ms: Some(0), text: "   ".into(), words: None },
            ],
        };
        let lrc = to_lrc(&payload);
        assert_eq!(lrc, "[1:01.23]第一句\n纯文本行\n");
        let back = parse(&lrc);
        assert_eq!(back.lines.len(), 2);
        assert_eq!(back.lines[0].time_ms, Some(61_230));
        assert!(back.synced);
    }
}
