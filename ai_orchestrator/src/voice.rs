// Everything that makes Aish *sound* like a person rather than a TTS demo:
// persona prompts, cleaning text so it reads well out loud, splitting it into
// breath-sized phrases, and pacing (speed + pauses) per phrase.
//
// Pure std on purpose, so it can be unit tested without llama.cpp or kokoro.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const SAMPLE_RATE: u32 = 24_000;

// Kokoro's most natural English voice. Override with AISH_VOICE=af_sarah etc.
pub const DEFAULT_VOICE: &str = "af_heart";

pub fn voice_name() -> String {
    std::env::var("AISH_VOICE").unwrap_or_else(|_| DEFAULT_VOICE.to_string())
}

// ---------------------------------------------------------------------------
// Persona
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mood {
    // A command just failed — be calm and reassuring, not cheerful.
    Failure,
    // The user asked something in chat.
    Chat,
}

static RECENT_LINES: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
const RECENT_LIMIT: usize = 6;

pub fn remember_spoken(line: &str) {
    if let Ok(mut recent) = RECENT_LINES.lock() {
        recent.push_back(line.to_string());
        while recent.len() > RECENT_LIMIT {
            recent.pop_front();
        }
    }
}

fn recent_lines() -> Vec<String> {
    RECENT_LINES
        .lock()
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default()
}

pub fn voice_system_prompt(mood: Mood, user_message: &str) -> String {
    let situation = match mood {
        Mood::Failure => {
            "A command the user ran just failed. React the way a calm, experienced \
             friend sitting next to them would: low-key and reassuring, maybe a little \
             dry humour, never cheerful about the error. Hint at what probably went \
             wrong in plain words. The detailed fix is shown on screen, so don't read it out."
                .to_string()
        }
        Mood::Chat => format!(
            "The user just said: \"{}\". Give the quick spoken reaction you'd say out \
             loud before the full answer appears on screen — acknowledge what they want, \
             or what you're about to do. Don't answer the whole thing.",
            user_message.chars().take(300).collect::<String>()
        ),
    };

    let recent = recent_lines();
    let avoid = if recent.is_empty() {
        String::new()
    } else {
        format!(
            "\nYou recently said these — don't reuse their openings or phrasing:\n{}",
            recent
                .iter()
                .map(|l| format!("- {}", l))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };

    format!(
        "You are Aish, the voice of the user's terminal. You talk like a real person, \
         not an assistant reading a script.\n\
         {situation}\n\
         Rules:\n\
         - One or two short sentences, under 25 words total.\n\
         - Casual and natural: contractions, plain words. An occasional \"hm\", \
           \"okay\" or \"right\" is fine; don't force it.\n\
         - No exclamation-mark pile-ups, no hype words (awesome, amazing, sweet), \
           no emoji, no markdown, no stage directions like *laughs*.\n\
         - Never read code, file paths, flags or long error text aloud.\n\
         - Output only the words you'd say.{avoid}"
    )
}

// Clean what the small model produced into a single spoken line.
pub fn tidy_spoken_line(raw: &str) -> String {
    let mut line = raw.trim();
    for prefix in ["Aish:", "aish:", "AISH:", "Assistant:", "Voice:"] {
        if let Some(rest) = line.strip_prefix(prefix) {
            line = rest.trim();
        }
    }
    // Models love to wrap the whole line in quotes.
    let line = line.trim_matches(|c| c == '"' || c == '\'' || c == '“' || c == '”');
    // Keep at most two sentences — anything longer starts sounding like a lecture.
    let sentences = split_phrases(line);
    let mut out = String::new();
    for s in sentences.iter().take(2) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(s);
    }
    to_speakable(&out)
}

// ---------------------------------------------------------------------------
// Text -> something a TTS engine reads naturally
// ---------------------------------------------------------------------------

pub fn to_speakable(text: &str) -> String {
    // 1. Drop fenced code blocks entirely — nobody wants shell syntax read aloud.
    let mut without_code = String::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            without_code.push_str(line);
            without_code.push(' ');
        }
    }

    // 2. Word-level rewrites.
    let mut words: Vec<String> = Vec::new();
    for raw_word in without_code.split_whitespace() {
        // Stage directions like *laughs* or *sigh* sound terrible when spoken.
        // (Single asterisks only — **word** is just bold.)
        let stage = raw_word.trim_end_matches(|c: char| ".,!?".contains(c));
        if stage.len() > 2 && stage.starts_with('*') && stage.ends_with('*') && !stage.starts_with("**") {
            continue;
        }
        let trailing: String = raw_word
            .chars()
            .rev()
            .take_while(|c| matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | ')'))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let core = &raw_word[..raw_word.len() - trailing.len()];
        let core = core.trim_matches(|c| matches!(c, '`' | '*' | '_' | '(' | '"' | '\''));

        let spoken = match core {
            "" => String::new(),
            "&&" => "and then".to_string(),
            "||" => "or else".to_string(),
            "|" => "piped into".to_string(),
            "->" | "=>" | "-" | "—" | "–" => ",".to_string(),
            "~" => "home".to_string(),
            "e.g" | "e.g." => "for example".to_string(),
            "i.e" | "i.e." => "that is".to_string(),
            "etc" => "and so on".to_string(),
            _ => speak_token(core),
        };
        if spoken.is_empty() && trailing.is_empty() {
            continue;
        }
        let trailing = trailing.replace(')', "");
        words.push(format!("{}{}", spoken, trailing));
    }

    // 3. Strip anything a TTS can't pronounce (emoji, box drawing, leftover symbols).
    let joined = words.join(" ");
    let cleaned: String = joined
        .chars()
        .filter(|c| c.is_alphanumeric() || " .,!?;:'-".contains(*c))
        .collect();

    // 4. Collapse the punctuation noise rewrites can leave behind (" , ,").
    let mut out = String::with_capacity(cleaned.len());
    for c in cleaned.chars() {
        if c == ',' && (out.ends_with(',') || out.ends_with(", ") || out.trim_end().is_empty()) {
            continue;
        }
        if c == ' ' && out.ends_with(' ') {
            continue;
        }
        if c == ',' && out.ends_with(' ') {
            out.pop();
        }
        out.push(c);
    }
    // Calm the hype: "!!!" -> "!", and at most one "!" per reply.
    let mut calm = String::with_capacity(out.len());
    let mut seen_bang = false;
    for c in out.chars() {
        if c == '!' {
            if seen_bang || calm.ends_with('!') {
                if !calm.ends_with('.') && !calm.ends_with('!') {
                    calm.push('.');
                }
                continue;
            }
            seen_bang = true;
        }
        calm.push(c);
    }
    calm.trim().trim_start_matches(|c| c == ',' || c == ' ').to_string()
}

fn speak_token(token: &str) -> String {
    // Paths: say only the last component ("src/audio.rs" -> "audio dot r s").
    let token = if token.contains('/') && token.len() > 1 {
        token.trim_end_matches('/').rsplit('/').next().unwrap_or(token)
    } else {
        token
    };
    // Flags: "--release" -> "release", "-la" -> "l a".
    if let Some(flag) = token.strip_prefix("--") {
        return flag.replace('-', " ");
    }
    if token.starts_with('-') && token.len() > 1 && token[1..].chars().all(|c| c.is_ascii_alphabetic()) {
        return spell(&token[1..]);
    }
    // File names: "main.rs" -> "main dot r s", "new.c" -> "new dot c".
    if let Some((stem, ext)) = token.rsplit_once('.') {
        let is_number = stem.chars().all(|c| c.is_ascii_digit()) && ext.chars().all(|c| c.is_ascii_digit());
        if !stem.is_empty() && !ext.is_empty() && !is_number && ext.len() <= 4 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            let ext_spoken = match ext {
                "md" => "markdown".to_string(),
                "py" => "pie".to_string(),
                "json" | "toml" | "yaml" | "html" | "lock" => ext.to_string(),
                _ => spell(ext),
            };
            return format!("{} dot {}", stem.replace(['_', '.'], " "), ext_spoken);
        }
    }
    token.replace('_', " ")
}

fn spell(s: &str) -> String {
    s.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// Phrasing and pacing
// ---------------------------------------------------------------------------

// Split into breath-sized phrases. Each one is synthesised separately so the
// first phrase starts playing quickly and we control the pauses between them.
pub fn split_phrases(text: &str) -> Vec<String> {
    let mut phrases = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        current.push(c);
        let next = chars.get(i + 1).copied();
        let at_boundary = matches!(c, '.' | '!' | '?' | ';')
            && next.map_or(true, |n| n.is_whitespace());
        // Long run-on sentence: breathe at the next comma.
        let long_comma = c == ',' && current.split_whitespace().count() >= 14;
        if at_boundary || long_comma {
            let p = current.trim().to_string();
            if !p.is_empty() {
                phrases.push(p);
            }
            current.clear();
        }
    }
    let rest = current.trim();
    if !rest.is_empty() {
        phrases.push(rest.to_string());
    }
    phrases
}

// Small deterministic wobble so consecutive phrases don't share an identical cadence.
fn wobble(text: &str, spread: f32) -> f32 {
    let hash = text
        .bytes()
        .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    ((hash % 1000) as f32 / 1000.0 - 0.5) * 2.0 * spread
}

// Kokoro speed where 1.0 is its natural rate.
pub fn speed_for(phrase: &str) -> f32 {
    let words = phrase.split_whitespace().count().max(1);
    let technical = phrase
        .split_whitespace()
        .filter(|w| w.contains(" dot ") || w.chars().any(|c| c.is_ascii_digit()) || w.len() == 1)
        .count();
    let mut speed: f32 = 1.0;
    if phrase.contains(" dot ") || technical as f32 / words as f32 > 0.25 {
        speed -= 0.08; // slow down a touch for names and numbers
    }
    if phrase.ends_with('?') {
        speed -= 0.03;
    }
    if words <= 3 {
        speed -= 0.04; // "Hm, okay." lands better unhurried
    }
    (speed + wobble(phrase, 0.03)).clamp(0.85, 1.1)
}

// Silence after a phrase, in milliseconds.
pub fn pause_after_ms(phrase: &str) -> u32 {
    let base: f32 = match phrase.chars().last() {
        Some('?') => 380.0,
        Some('.') => 300.0,
        Some('!') => 260.0,
        Some(';') | Some(':') => 220.0,
        Some(',') => 160.0,
        _ => 200.0,
    };
    (base * (1.0 + wobble(phrase, 0.15))) as u32
}

// Trim the near-silent head/tail Kokoro leaves on each clip so our own pauses
// decide the rhythm, then fade the edges so clips join without clicks.
pub fn shape_clip(mut audio: Vec<f32>) -> Vec<f32> {
    const THRESHOLD: f32 = 0.004;
    let pad = (SAMPLE_RATE as usize) / 100 * 3; // keep 30ms of breath
    let start = audio.iter().position(|s| s.abs() > THRESHOLD).unwrap_or(0);
    let end = audio
        .iter()
        .rposition(|s| s.abs() > THRESHOLD)
        .map_or(audio.len(), |i| i + 1);
    let start = start.saturating_sub(pad);
    let end = (end + pad).min(audio.len());
    if start < end {
        audio = audio[start..end].to_vec();
    }

    let fade = ((SAMPLE_RATE as usize) / 1000 * 8).min(audio.len() / 2); // 8ms
    let len = audio.len();
    for i in 0..fade {
        let g = i as f32 / fade as f32;
        audio[i] *= g;
        audio[len - 1 - i] *= g;
    }
    audio
}

pub fn silence_ms(ms: u32) -> Vec<f32> {
    vec![0.0; (SAMPLE_RATE as usize) * ms as usize / 1000]
}

// ---------------------------------------------------------------------------
// Instant lines that don't need the LLM
// ---------------------------------------------------------------------------

fn pick<'a>(pool: &[&'a str]) -> &'a str {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let recent = recent_lines();
    // Prefer a line we haven't said lately.
    let start = nanos as usize % pool.len();
    for offset in 0..pool.len() {
        let candidate = pool[(start + offset) % pool.len()];
        if !recent.iter().any(|r| r.starts_with(&candidate[..candidate.len().min(12)])) {
            return candidate;
        }
    }
    pool[start]
}

pub fn greeting(hour: u32) -> String {
    let pool: &[&str] = match hour {
        5..=11 => &["Morning. I'm up whenever you are.", "Hey, good morning. Ready when you are."],
        12..=17 => &["Hey. I'm here.", "Afternoon. What are we working on?"],
        18..=22 => &["Evening. I'm all set.", "Hey. Ready when you are."],
        _ => &["Late one, huh? I'm here.", "Still up? Okay, I'm ready."],
    };
    pick(pool).to_string()
}

pub fn spoken_duration(seconds: u64) -> String {
    match seconds {
        0..=44 => "under a minute".to_string(),
        45..=89 => "about a minute".to_string(),
        90..=3599 => format!("about {} minutes", (seconds + 30) / 60),
        _ => {
            let hours = seconds / 3600;
            if hours == 1 { "over an hour".to_string() } else { format!("over {} hours", hours) }
        }
    }
}

// Spoken when a long-running command finishes and the user may have looked away.
pub fn command_finished_line(command: &str, exit_code: i32, seconds: u64) -> String {
    let what = command
        .split_whitespace()
        .next()
        .map(speak_token)
        .unwrap_or_else(|| "That".to_string());
    let took = spoken_duration(seconds);
    let template = if exit_code == 0 {
        pick(&[
            "{what} is done. Took {took}.",
            "Okay, {what} finished. That was {took}.",
            "{what}'s wrapped up, {took}.",
            "Done with {what}. All clean.",
        ])
    } else {
        pick(&[
            "Hm, {what} stopped with an error after {took}.",
            "{what} didn't make it, it failed after {took}.",
            "Heads up, {what} exited with code {code}.",
        ])
    };
    template
        .replace("{what}", &what)
        .replace("{took}", &took)
        .replace("{code}", &exit_code.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markdown_and_code() {
        let s = to_speakable("Run this:\n```sh\nrm -rf build\n```\nThen **retry** `make`.");
        assert!(!s.contains("rm"));
        assert!(!s.contains('*'));
        assert!(!s.contains('`'));
        assert!(s.contains("retry"));
    }

    #[test]
    fn paths_and_files_read_naturally() {
        assert_eq!(to_speakable("Check src/audio.rs now."), "Check audio dot r s now.");
        assert_eq!(to_speakable("Open notes.md"), "Open notes dot markdown");
        assert_eq!(to_speakable("version 1.5 works"), "version 1.5 works");
        assert_eq!(to_speakable("use --release"), "use release");
    }

    #[test]
    fn stage_directions_and_hype_are_removed() {
        let s = to_speakable("*laughs* Oh wow!!! That's great!");
        assert!(!s.contains("laughs"));
        assert_eq!(s.matches('!').count(), 1);
    }

    #[test]
    fn phrases_split_on_sentences() {
        let p = split_phrases("Okay. That failed because the file is missing! Want me to check?");
        assert_eq!(p.len(), 3);
        assert_eq!(p[2], "Want me to check?");
        // Decimals don't split.
        assert_eq!(split_phrases("Version 1.5 is out.").len(), 1);
    }

    #[test]
    fn tidy_removes_quotes_and_prefixes() {
        assert_eq!(tidy_spoken_line("Aish: \"Hm, typo in the path.\""), "Hm, typo in the path.");
        let long = tidy_spoken_line("One. Two. Three. Four.");
        assert_eq!(long, "One. Two.");
    }

    #[test]
    fn pacing_is_in_range() {
        for p in ["Hm.", "That's the main dot r s file.", "Want me to look?"] {
            let s = speed_for(p);
            assert!((0.85..=1.1).contains(&s), "{p} -> {s}");
            assert!(pause_after_ms(p) > 100);
        }
    }

    #[test]
    fn clip_shaping_trims_and_fades() {
        let mut clip = vec![0.0; 24_000];
        clip.extend(vec![0.5; 4800]);
        clip.extend(vec![0.0; 24_000]);
        let shaped = shape_clip(clip);
        assert!(shaped.len() < 10_000);
        assert_eq!(shaped[0], 0.0);
        assert_eq!(*shaped.last().unwrap(), 0.0);
    }

    #[test]
    fn finished_lines() {
        let ok = command_finished_line("cargo build --release", 0, 95);
        assert!(ok.contains("cargo"), "{ok}");
        let bad = command_finished_line("make", 2, 30);
        assert!(bad.contains("make"), "{bad}");
        assert_eq!(spoken_duration(3700), "over an hour");
    }
}
