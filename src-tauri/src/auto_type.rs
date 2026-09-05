//! Append only stable speech. Never backspace over text the user may have edited.
use std::collections::BTreeMap;

#[derive(Default)]
pub struct AutoTypeSession {
    chunks: BTreeMap<u64, Chunk>,
}

#[derive(Default)]
struct Chunk {
    previous: String,
    stable: String,
    typed: String,
    final_text: Option<String>,
}

impl AutoTypeSession {
    pub fn start(&mut self, id: u64) -> Result<(), String> {
        if self.chunks.len() >= 32 {
            return Err("Auto-type is falling behind. Choose a faster model or check Activity for failed transcription.".into());
        }
        self.chunks.entry(id).or_default();
        Ok(())
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    pub fn partial(&mut self, id: u64, text: &str) {
        let Some(chunk) = self.chunks.get_mut(&id) else {
            return;
        };
        if chunk.final_text.is_some() {
            return;
        }
        let text = text.trim_start();
        let prefix: String = chunk
            .previous
            .chars()
            .zip(text.chars())
            .take_while(|(left, right)| left == right)
            .map(|(ch, _)| ch)
            .collect();
        // Wait for a word boundary when available. CJK can make progress without spaces.
        let boundary = prefix
            .char_indices()
            .rev()
            .find(|(_, ch)| ch.is_whitespace())
            .map(|(index, ch)| index + ch.len_utf8())
            .unwrap_or(prefix.len());
        let stable = &prefix[..boundary];
        if stable.starts_with(&chunk.typed) && stable.len() > chunk.stable.len() {
            chunk.stable = stable.to_string();
        }
        chunk.previous = text.to_string();
    }

    pub fn finish(&mut self, id: u64, text: &str) {
        if let Some(chunk) = self.chunks.get_mut(&id) {
            chunk.final_text = Some(text.trim().to_string());
        }
    }

    pub fn discard(&mut self, id: u64) {
        self.chunks.remove(&id);
    }

    pub fn flush(
        &mut self,
        trailing_space: bool,
        mut type_text: impl FnMut(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        while let Some((&id, chunk)) = self.chunks.first_key_value() {
            let is_final = chunk.final_text.is_some();
            let target = chunk.final_text.as_ref().unwrap_or(&chunk.stable);
            let Some(suffix) = target.strip_prefix(&chunk.typed) else {
                self.chunks.remove(&id);
                return Err("Auto-type stopped this phrase because the model revised words already typed. The complete corrected transcript is available in Activity.".into());
            };
            let mut suffix = suffix.to_string();
            if is_final && trailing_space && !target.is_empty() {
                suffix.push(' ');
            }
            if !suffix.is_empty() {
                type_text(&suffix)?;
            }
            if is_final {
                self.chunks.remove(&id);
            } else {
                self.chunks.get_mut(&id).expect("active chunk").typed = target.clone();
                break;
            }
        }
        Ok(())
    }
}
