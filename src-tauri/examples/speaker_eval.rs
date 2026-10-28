//! Compare speaker embedding models through the same path the app uses.
//!
//! Layout: `<eval-root>/<lang>/<speaker>/<utterance>.wav`. The first
//! `ENROLL_UTTERANCES` files of each speaker stand in for the enrollment
//! sentences; later files are cut to clip lengths and scored against every
//! profile of the same language.
//!
//! cargo run --release --features asr-sherpa --example speaker_eval -- <eval-root> <model.onnx>...
//! cargo run --release --features asr-sherpa --example speaker_eval -- --time <eval-root> <model.onnx>...
//!
//! `WAKENOTE_EVAL_SCORES=<dir>` also writes every trial score there
//! (`t`/`n` for same/other speaker) to compare thresholds across languages.
//!
//! `--time` also transcribes the same 5 s clips with a Whisper model when
//! `WAKENOTE_COMPARE_WHISPER=<model.bin>` is set, to compare against judging.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use wakenote::dictionary::DictionaryContext;
use wakenote::settings::TranscriptionLanguage;
use wakenote::speaker::{SAMPLE_RATE, SpeakerEmbedder, cosine_similarity};
use wakenote::transcription::{
    cached_whisper_context, decode_audio_for_whisper, transcribe_samples_with_context,
};

const ENROLL_UTTERANCES: usize = 5;
const TEST_UTTERANCES: usize = 20;
const CLIP_SECONDS: [f32; 3] = [1.5, 3.0, 5.0];
const TIMING_CLIPS: usize = 20;

struct Speaker {
    files: Vec<PathBuf>,
}

fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let timing = args.first().is_some_and(|arg| arg == "--time");
    if timing {
        args.remove(0);
    }
    let (root, models) = args
        .split_first()
        .expect("usage: speaker_eval [--time] <eval-root> <model.onnx>...");
    let root = PathBuf::from(root);
    for model in models {
        let model = PathBuf::from(model);
        if timing {
            time_model(&root, &model);
        } else {
            evaluate_model(&root, &model);
        }
    }
    if timing && let Ok(whisper) = std::env::var("WAKENOTE_COMPARE_WHISPER") {
        time_whisper(&root, Path::new(&whisper));
    }
}

fn evaluate_model(root: &Path, model: &Path) {
    let mut embedder = SpeakerEmbedder::load(model).expect("load model");
    let name = model.file_stem().unwrap().to_string_lossy();
    for language in subdirectories(root) {
        let speakers = speakers(&language);
        let mut profiles = Vec::new();
        let mut tests: Vec<Vec<(usize, Vec<f32>)>> = vec![Vec::new(); CLIP_SECONDS.len()];
        for (index, speaker) in speakers.iter().enumerate() {
            let (enroll, rest) = speaker
                .files
                .split_at(ENROLL_UTTERANCES.min(speaker.files.len()));
            let mut centroid = Vec::new();
            for file in enroll {
                let embedding = embedder.embed(&decode(file)).expect("enroll embedding");
                if centroid.is_empty() {
                    centroid = vec![0.0; embedding.len()];
                }
                centroid
                    .iter_mut()
                    .zip(&embedding)
                    .for_each(|(sum, value)| *sum += value);
            }
            profiles.push(centroid);
            for file in rest.iter().take(TEST_UTTERANCES) {
                let samples = decode(file);
                for (slot, seconds) in CLIP_SECONDS.iter().enumerate() {
                    let length = (seconds * SAMPLE_RATE as f32) as usize;
                    if samples.len() < length {
                        continue;
                    }
                    let embedding = embedder.embed(&samples[..length]).expect("test embedding");
                    tests[slot].push((index, embedding));
                }
            }
        }
        for (slot, seconds) in CLIP_SECONDS.iter().enumerate() {
            let (mut targets, mut nontargets) = (Vec::new(), Vec::new());
            for (owner, profile) in profiles.iter().enumerate() {
                for (speaker, embedding) in &tests[slot] {
                    let score = cosine_similarity(profile, embedding);
                    if *speaker == owner {
                        targets.push(score);
                    } else {
                        nontargets.push(score);
                    }
                }
            }
            if let Ok(dir) = std::env::var("WAKENOTE_EVAL_SCORES") {
                let lines = targets
                    .iter()
                    .map(|score| format!("t {score}"))
                    .chain(nontargets.iter().map(|score| format!("n {score}")))
                    .collect::<Vec<_>>();
                let file = format!(
                    "{name}-{}-{seconds:.1}.txt",
                    language.file_name().unwrap().to_string_lossy()
                );
                fs::write(Path::new(&dir).join(file), lines.join("\n")).expect("write scores");
            }
            let (eer, eer_threshold) = equal_error_rate(&targets, &nontargets);
            let (strict_threshold, strict_miss) =
                miss_rate_at_false_accept(&targets, &nontargets, 0.01);
            println!(
                "| {name} | {lang} | {seconds:.1}s | {speakers} | {targets} / {nontargets} | {eer:.2}% | {eer_threshold:.3} | {strict_miss:.2}% @ {strict_threshold:.3} |",
                lang = language.file_name().unwrap().to_string_lossy(),
                speakers = speakers.len(),
                targets = targets.len(),
                nontargets = nontargets.len(),
                eer = eer * 100.0,
                strict_miss = strict_miss * 100.0,
            );
        }
    }
}

fn time_model(root: &Path, model: &Path) {
    let started = Instant::now();
    let mut embedder = SpeakerEmbedder::load(model).expect("load model");
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    let clips = timing_clips(root);
    embedder.embed(&clips[0]).expect("warm-up embedding");
    let started = Instant::now();
    for clip in &clips {
        embedder.embed(clip).expect("timed embedding");
    }
    let embed_ms = started.elapsed().as_secs_f64() * 1000.0 / clips.len() as f64;
    println!(
        "| {} | {} | {load_ms:.0} ms | {embed_ms:.1} ms |",
        model.file_stem().unwrap().to_string_lossy(),
        embedder.dimension(),
    );
}

fn time_whisper(root: &Path, model: &Path) {
    let context = cached_whisper_context(model).expect("load whisper");
    let dictionary = DictionaryContext::default();
    let clips = timing_clips(root);
    let transcribe = |clip: &[f32]| {
        transcribe_samples_with_context(
            &context,
            clip,
            TranscriptionLanguage::Auto,
            false,
            &dictionary,
            |_| {},
        )
        .expect("whisper transcription");
    };
    transcribe(&clips[0]);
    let started = Instant::now();
    for clip in &clips {
        transcribe(clip);
    }
    let transcribe_ms = started.elapsed().as_secs_f64() * 1000.0 / clips.len() as f64;
    println!(
        "| {} (transcription) | - | - | {transcribe_ms:.1} ms |",
        model.file_stem().unwrap().to_string_lossy(),
    );
}

/// The first `TIMING_CLIPS` utterances that fill a 5 s clip, cut to 5 s.
fn timing_clips(root: &Path) -> Vec<Vec<f32>> {
    let length = 5 * SAMPLE_RATE as usize;
    subdirectories(root)
        .iter()
        .flat_map(|language| speakers(language))
        .flat_map(|speaker| speaker.files)
        .map(|file| decode(&file))
        .filter(|samples| samples.len() >= length)
        .take(TIMING_CLIPS)
        .map(|mut samples| {
            samples.truncate(length);
            samples
        })
        .collect()
}

/// EER and the threshold where false accepts and misses meet.
fn equal_error_rate(targets: &[f32], nontargets: &[f32]) -> (f32, f32) {
    let mut best = (f32::MAX, 1.0, 0.0);
    for &threshold in targets.iter().chain(nontargets) {
        let false_accept = rate(nontargets, |score| score >= threshold);
        let miss = rate(targets, |score| score < threshold);
        let gap = (false_accept - miss).abs();
        if gap < best.0 {
            best = (gap, (false_accept + miss) / 2.0, threshold);
        }
    }
    (best.1, best.2)
}

/// Lowest threshold that lets at most `limit` of other speakers through, and
/// the share of the speaker's own clips it rejects.
fn miss_rate_at_false_accept(targets: &[f32], nontargets: &[f32], limit: f32) -> (f32, f32) {
    let mut sorted = nontargets.to_vec();
    sorted.sort_by(|a, b| b.total_cmp(a));
    let allowed = (nontargets.len() as f32 * limit).floor() as usize;
    let threshold = sorted
        .get(allowed)
        .map(|score| score + f32::EPSILON)
        .unwrap_or(f32::MIN);
    (threshold, rate(targets, |score| score < threshold))
}

fn rate(scores: &[f32], hit: impl Fn(f32) -> bool) -> f32 {
    scores.iter().filter(|score| hit(**score)).count() as f32 / scores.len().max(1) as f32
}

fn decode(path: &Path) -> Vec<f32> {
    decode_audio_for_whisper(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn subdirectories(path: &Path) -> Vec<PathBuf> {
    let mut entries = fs::read_dir(path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn speakers(language: &Path) -> Vec<Speaker> {
    subdirectories(language)
        .into_iter()
        .map(|dir| {
            let mut files = fs::read_dir(&dir)
                .unwrap()
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.extension().is_some_and(|ext| ext == "wav"))
                .collect::<Vec<_>>();
            files.sort();
            Speaker { files }
        })
        .filter(|speaker| speaker.files.len() > ENROLL_UTTERANCES)
        .collect()
}
