use std::path::{Path, PathBuf};

use crate::engine::{Segment, TranscribeOptions};

mod t2s;

/// 确定 voice2text 输出目录：在 output_base 下新建 voice2text/
pub fn output_dir(output_base: &Path) -> PathBuf {
    output_base.join("voice2text")
}

/// 自动分段排版 + 生成 MD 表格、SRT、VTT、TXT，写入 voice2text 目录。
/// 返回该文件结果所在目录。
pub fn write_outputs(
    input: &Path,
    segments: &[Segment],
    opts: &TranscribeOptions,
    output_base: &Path,
) -> Result<PathBuf, String> {
    let dir = output_dir(output_base);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("out");

    let paragraphs = segment_paragraphs(segments);
    let paragraphs: Vec<String> = paragraphs.into_iter().map(|p| t2s::simplify(&p)).collect();
    let segs: Vec<Segment> = segments
        .iter()
        .map(|s| Segment {
            start: s.start,
            end: s.end,
            text: t2s::simplify(&s.text),
        })
        .collect();
    let formats = &opts.output_formats;
    let want = |f: &str| formats.iter().any(|x| x == f) || formats.is_empty();

    if want("md") {
        let md = build_markdown(stem, &segs, &paragraphs);
        std::fs::write(dir.join(format!("{}.md", stem)), md).map_err(|e| e.to_string())?;
    }
    if want("srt") {
        let srt = build_srt(&segs);
        std::fs::write(dir.join(format!("{}.srt", stem)), srt).map_err(|e| e.to_string())?;
    }
    if want("vtt") {
        let vtt = build_vtt(&segs);
        std::fs::write(dir.join(format!("{}.vtt", stem)), vtt).map_err(|e| e.to_string())?;
    }
    if want("txt") {
        let txt = paragraphs.join("\n\n");
        std::fs::write(dir.join(format!("{}.txt", stem)), txt).map_err(|e| e.to_string())?;
    }
    if want("prompt") {
        let prompt_html = build_teleprompter_html(stem, &paragraphs);
        std::fs::write(dir.join(format!("{}-提词稿.html", stem)), prompt_html).map_err(|e| e.to_string())?;
        let prompt_md = build_teleprompter(stem, &paragraphs);
        std::fs::write(dir.join(format!("{}-提词稿.md", stem)), prompt_md).map_err(|e| e.to_string())?;
    }
    if want("rtf") {
        let rtf = build_rtf(stem, &paragraphs);
        std::fs::write(dir.join(format!("{}.rtf", stem)), rtf).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

/// 按句末标点 + 静音间隔，将片段自动合并为自然段落
fn segment_paragraphs(segments: &[Segment]) -> Vec<String> {
    if segments.is_empty() {
        return vec![];
    }
    let mut paras: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_end: f64 = -1.0;

    for seg in segments {
        let text = seg.text.trim();
        if text.is_empty() {
            continue;
        }
        let gap = if prev_end < 0.0 { 0.0 } else { seg.start - prev_end };
        let ends_sentence = cur.ends_with('。')
            || cur.ends_with('！')
            || cur.ends_with('？')
            || cur.ends_with('.')
            || cur.ends_with('!')
            || cur.ends_with('?')
            || cur.ends_with('…');
        let too_long = cur.chars().count() > 120;

        let new_para = !cur.is_empty()
            && ((ends_sentence && (gap > 1.0 || too_long)) || gap > 2.5);

        if new_para {
            paras.push(cur.trim().to_string());
            cur = String::new();
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(text);
        prev_end = seg.end;
    }
    if !cur.is_empty() {
        paras.push(cur.trim().to_string());
    }
    paras
}

fn fmt_time(t: f64) -> String {
    let total = t as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}

fn srt_time(t: f64) -> String {
    let total_ms = (t * 1000.0) as u64;
    let h = total_ms / 3_600_000;
    let m = (total_ms % 3_600_000) / 60_000;
    let sec = (total_ms % 60_000) / 1000;
    let ms = total_ms % 1000;
    format!("{}:{:02}:{:02},{:03}", h, m, sec, ms)
}

fn vtt_time(t: f64) -> String {
    let total_ms = (t * 1000.0) as u64;
    let h = total_ms / 3_600_000;
    let m = (total_ms % 3_600_000) / 60_000;
    let sec = (total_ms % 60_000) / 1000;
    let ms = total_ms % 1000;
    format!("{}:{:02}:{:02}.{:03}", h, m, sec, ms)
}

fn build_markdown(stem: &str, segments: &[Segment], paragraphs: &[String]) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", stem));
    md.push_str("## 转写全文\n\n");
    for p in paragraphs {
        md.push_str(&format!("{}\n\n", p));
    }
    md.push_str("## 时间轴（表格）\n\n");
    md.push_str("| 时间区间 | 文本 |\n");
    md.push_str("| --- | --- |\n");
    for seg in segments {
        let t = format!("[{} - {}]", fmt_time(seg.start), fmt_time(seg.end));
        let text = seg.text.replace('|', "\\|").replace('\n', " ");
        md.push_str(&format!("| {} | {} |\n", t, text));
    }
    md
}

fn build_srt(segments: &[Segment]) -> String {
    let mut s = String::new();
    for (i, seg) in segments.iter().enumerate() {
        s.push_str(&format!("{}\n", i + 1));
        s.push_str(&format!("{} --> {}\n", srt_time(seg.start), srt_time(seg.end)));
        s.push_str(&format!("{}\n\n", seg.text.trim()));
    }
    s
}

/// 纯文本 MD 提词稿（每行 ≤12 字）
fn build_teleprompter(_stem: &str, paragraphs: &[String]) -> String {
    let mut out = String::new();
    for p in paragraphs {
        for line in split_teleprompter_lines(p, 12) {
            out.push_str(&line);
            out.push('\n');
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn build_teleprompter_html(stem: &str, paragraphs: &[String]) -> String {
    let title = html_escape(stem);
    let mut body = String::new();
    for p in paragraphs {
        body.push_str("      <p>");
        let lines = split_teleprompter_lines(p, 12);
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                body.push_str("<br>");
            }
            body.push_str(&html_escape(line));
        }
        body.push_str("</p>\n");
    }
    format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title} · 提词稿</title>
<style>
  *,*::before,*::after{{margin:0;padding:0;box-sizing:border-box}}
  html{{height:100%}}
  body{{
    min-height:100%;
    background:#0f172a;
    color:#f1f5f9;
    font-family:-apple-system,"PingFang SC","Microsoft YaHei","Noto Sans SC",sans-serif;
    display:flex;justify-content:center;align-items:center;
    padding:3rem 2rem;
  }}
  .prompter{{
    max-width:900px;width:100%;text-align:center;
  }}
  .prompter p{{
    font-size:clamp(22px,6vw,48px);
    line-height:2.6;
    letter-spacing:0.14em;
    font-weight:500;
    margin:0 0 2em;
    word-break:keep-all;
  }}
  .prompter p:last-child{{margin-bottom:0}}
</style>
</head>
<body>
<div class="prompter">
{body}</div>
<script>
(function(){{
  var speed=1,interval=30;
  var max=document.body.scrollHeight-window.innerHeight,cur=0;
  function step(){{
    cur+=speed;if(cur>=max)cur=0;
    window.scrollTo(0,cur);
    setTimeout(step,interval);
  }}
  step();
}})();
</script>
</body>
</html>"#,
        title = title,
        body = body,
    )
}

/// RTF 编码器：生成 Word 可直接打开的 .doc 文件（跨平台默认中文字体，不硬编码字体名）
fn build_rtf(_stem: &str, paragraphs: &[String]) -> String {
    let mut body = String::new();
    for p in paragraphs {
        body.push_str("\\pard\\qc\\sa240\\sl480\\slmult1{\\fs44 ");
        body.push_str(&rtf_text(p));
        body.push_str("\\par}\n");
    }
    format!(
        "{{\\rtf1\\ansi\\deff0\n\
         {{\\fonttbl{{\\f0\\fnil\\fcharset134;}}}}\n\
         \\f0\\fs44\\lang2052\n\
         \\paperw11906\\paperh16838\\margl1440\\margr1440\\margt1440\\margb1440\n\
         {body}}}\n",
        body = body,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn rtf_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' => out.push_str("\\line "),
            c if c as u32 <= 127 => out.push(c),
            c => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{}?", c as u32);
            }
        }
    }
    out
}

/// 将一段中文文本按每行 ≤max_chars 拆为多行，尽量在标点处断行
fn split_teleprompter_lines(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    let punct = [',', '，', '、', '；', '。', '！', '？', '.', '!', '?', '…', ' ', ':', '：'];
    let hard_punct = ['。', '！', '？', '.', '!', '?', '…'];

    for ch in text.chars() {
        cur.push(ch);
        if cur.chars().count() > max_chars {
            // 回找最近的标点作为断行点（按字符索引，不是字节偏移）
            let chars_vec: Vec<char> = cur.chars().collect();
            let char_count = chars_vec.len();
            let break_at = chars_vec
                .iter()
                .enumerate()
                .rev()
                .skip(1) // 跳过刚 push 的当前字符
                .find(|(_, c)| punct.contains(*c) || **c == ' ')
                .map(|(i, _)| i + 1)
                .unwrap_or(max_chars.min(char_count.saturating_sub(1)));
            let mut line: String = chars_vec.iter().take(break_at).collect();
            line = line.trim_end().to_string();
            if !line.is_empty() {
                lines.push(line);
            }
            let rest: String = chars_vec.iter().skip(break_at).collect();
            cur = rest;
        }
        // 在强断句标点后主动断行
        if hard_punct.contains(&ch) && cur.chars().count() >= max_chars / 2 {
            lines.push(cur.trim().to_string());
            cur.clear();
        }
    }
    let rem = cur.trim().to_string();
    if !rem.is_empty() {
        lines.push(rem);
    }
    lines
}

fn build_vtt(segments: &[Segment]) -> String {
    let mut s = String::from("WEBVTT\n\n");
    for (_i, seg) in segments.iter().enumerate() {
        s.push_str(&format!("{} --> {}\n", vtt_time(seg.start), vtt_time(seg.end)));
        s.push_str(&format!("{}\n\n", seg.text.trim()));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vtt_uses_arrow_separator() {
        let segs = vec![Segment { start: 0.0, end: 1.5, text: "hello".into() }];
        let out = build_vtt(&segs);
        assert!(out.contains(" --> "), "VTT must use ' --> ' per WebVTT spec, got:\n{}", out);
        assert!(!out.contains("0:00:00.000 - "), "must not use bare hyphen separator");
    }

    #[test]
    fn teleprompter_splits_chinese_by_char_count() {
        // 40 中文字符，无标点，max_chars=12 → 至少要拆成 3+ 行
        let text = "一二三四五六七八九十甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥春夏秋冬东西南北";
        let lines = split_teleprompter_lines(text, 12);
        assert!(lines.len() >= 3, "40-char Chinese text must split into ≥3 lines at max=12, got {} lines: {:?}", lines.len(), lines);
        for line in &lines {
            assert!(
                line.chars().count() <= 14,
                "each line should stay near max_chars=12, got {} chars: {:?}",
                line.chars().count(),
                line
            );
        }
    }

    #[test]
    fn teleprompter_breaks_at_punctuation() {
        let text = "今天天气不错，我们出去散步吧，顺便买点东西。";
        let lines = split_teleprompter_lines(text, 12);
        // 应该在逗号/句号处断行
        assert!(lines.len() >= 2, "should split at punctuation: {:?}", lines);
    }
}
