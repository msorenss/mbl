//! Raspberry Pi 5 Hailo-8 / Hailo-10H NPU media intelligence (`mbl-hailo`, M5.13).
//!
//! Integrates with [`mbl_hwaccel::hailo`] (ADR 0006 cooperative `VDevice` group sharing) so MBL can
//! run semantic visual scene embeddings, face/object/scene tagging, and `hailo-ollama` spoiler-free
//! recaps alongside co-tenants such as Frigate.

pub mod runtime;

use std::path::Path;
use std::time::Instant;

use image::{DynamicImage, GenericImageView};
use mbl_hwaccel::NpuProfile;
use mbl_hwaccel::fingerprint::detect_fingerprint_from_root;
use mbl_hwaccel::hailo::{DutyCyclePacer, HailoConfig, HailoRuntimeStatus, NpuCoordinator};
use runtime::{HailoNativeRuntime, map_detections_to_tags_and_faces};
use serde::{Deserialize, Serialize};

/// Dimension of the L2-normalized multimodal scene/text embedding vector.
pub const EMBEDDING_DIM: usize = 64;

/// Semantic concept vocabulary anchored in the first 24 dimensions of the multimodal space.
const CONCEPTS: &[(&str, &[&str])] = &[
    ("snow", &["snow", "winter", "ice", "blizzard", "cold", "ski", "frost", "arctic"]),
    ("car_chase", &["car", "chase", "driving", "highway", "vehicle", "race", "pursuit", "road"]),
    ("night", &["night", "dark", "midnight", "shadow", "moonlight", "noir"]),
    ("fire", &["fire", "explosion", "flame", "burning", "smoke", "blast"]),
    ("ocean", &["ocean", "sea", "water", "beach", "waves", "boat", "ship", "underwater"]),
    ("forest", &["forest", "trees", "woods", "jungle", "nature", "leaves", "trail"]),
    ("mountain", &["mountain", "peak", "cliff", "alpine", "summit", "ridge"]),
    ("city", &["city", "street", "building", "downtown", "skyline", "urban", "traffic"]),
    ("space", &["space", "starship", "galaxy", "planet", "orbit", "astronaut", "cosmos"]),
    ("rain", &["rain", "storm", "wet", "umbrella", "thunder", "lightning"]),
    ("sunset", &["sunset", "sunrise", "dusk", "dawn", "golden", "horizon"]),
    ("indoor", &["room", "office", "kitchen", "house", "table", "interior", "hallway"]),
    ("face", &["face", "person", "portrait", "closeup", "people", "talking", "dialogue"]),
    ("action", &["fight", "battle", "combat", "running", "jump", "weapon", "chase"]),
    ("aerial", &["aerial", "drone", "flying", "sky", "clouds", "helicopter"]),
    ("desert", &["desert", "sand", "dunes", "arid", "canyon", "heat"]),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FrameAnalysis {
    pub tags: Vec<String>,
    pub face_count: u32,
    pub caption: String,
    pub embedding: Vec<f32>,
    pub backend: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HailoEmbeddingBenchmark {
    pub backend: String,
    pub npu_profile: NpuProfile,
    pub device_nodes: Vec<String>,
    pub sharing_mode: String,
    pub vdevice_group_id: String,
    pub active_co_tenants: usize,
    pub iterations: usize,
    pub elapsed_ms: f64,
    pub frames_per_sec: f64,
    pub avg_latency_us: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WatchedEpisodeContext {
    pub season: u32,
    pub episode: u32,
    pub title: String,
    pub summary: String,
    pub watched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpoilerFreeRecap {
    pub show_title: String,
    pub episodes_included: usize,
    pub spoiler_protected_unwatched_excluded: usize,
    pub recap_text: String,
    pub model_backend: String,
}

/// Runtime handle for Hailo NPU & `hailo-ollama` inference.
#[derive(Debug, Clone)]
pub struct HailoEngine {
    config: HailoConfig,
    ollama_url: String,
    runtime: HailoNativeRuntime,
}

impl Default for HailoEngine {
    fn default() -> Self {
        Self::new(HailoConfig::default())
    }
}

impl HailoEngine {
    pub fn new(config: HailoConfig) -> Self {
        let ollama_url = std::env::var("MBL_HAILO_OLLAMA_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8000".to_owned());
        let runtime = HailoNativeRuntime::new(&config);
        Self { config, ollama_url, runtime }
    }

    pub fn with_ollama_url(mut self, url: impl Into<String>) -> Self {
        self.ollama_url = url.into();
        self
    }

    /// Inspect live Hailo NPU status on `/` (or a custom root in tests).
    pub fn runtime_status(&self) -> HailoRuntimeStatus {
        self.runtime_status_from_root(Path::new("/"))
    }

    pub fn runtime_status_from_root(&self, root: &Path) -> HailoRuntimeStatus {
        let fp = detect_fingerprint_from_root(root, std::env::consts::ARCH, None);
        NpuCoordinator::new(self.config.clone()).status(&fp, None)
    }

    /// Returns the active inference backend identifier:
    /// `"hailort-npu(<ARCH>,<MODEL.hef>)"` when a Hailo-8/8L/10H NPU session is loaded,
    /// or `"cpu-heuristic"` when running without an NPU or `.hef` model.
    pub fn active_backend_name(&self) -> String {
        self.runtime.active_backend_name().unwrap_or_else(|| "cpu-heuristic".into())
    }

    /// Analyze a video keyframe or photo: execute Hailo NPU inference when available,
    /// extract scene/object tags, estimate face/person regions, synthesize a descriptive caption,
    /// and compute a 64-dim L2-normalized semantic embedding.
    pub fn analyze_image(&self, img: &DynamicImage, hint_text: Option<&str>) -> FrameAnalysis {
        let (w, h) = img.dimensions();
        let mut r_sum = 0.0f64;
        let mut g_sum = 0.0f64;
        let mut b_sum = 0.0f64;
        let mut bright_white_pixels = 0u32;
        let mut dark_pixels = 0u32;
        let mut skin_tone_pixels = 0u32;
        let mut green_pixels = 0u32;
        let mut blue_pixels = 0u32;
        let mut warm_fire_pixels = 0u32;

        let step_x = (w / 32).max(1);
        let step_y = (h / 32).max(1);
        let mut samples = 0u32;
        let mut spatial_bins = [0.0f32; 32];

        for sy in (0..h).step_by(step_y as usize) {
            for sx in (0..w).step_by(step_x as usize) {
                let px = img.get_pixel(sx, sy).0;
                let (r, g, b) = (f64::from(px[0]), f64::from(px[1]), f64::from(px[2]));
                r_sum += r;
                g_sum += g;
                b_sum += b;
                samples += 1;

                let luma = 0.299 * r + 0.587 * g + 0.114 * b;
                let bin_idx = (((sy * 4 / h.max(1)) * 8) + (sx * 8 / w.max(1))).min(31) as usize;
                spatial_bins[bin_idx] += (luma as f32) / 255.0;

                if r > 205.0 && g > 205.0 && b > 210.0 {
                    bright_white_pixels += 1;
                }
                if luma < 42.0 {
                    dark_pixels += 1;
                }
                if r > 95.0
                    && g > 40.0
                    && b > 20.0
                    && r > g
                    && r > b
                    && (r - g) > 15.0
                    && (r - b) < 110.0
                {
                    skin_tone_pixels += 1;
                }
                if g > r * 1.15 && g > b * 1.10 && g > 60.0 {
                    green_pixels += 1;
                }
                if b > r * 1.20 && b > g * 1.05 && b > 70.0 {
                    blue_pixels += 1;
                }
                if r > 200.0 && g > 80.0 && g < 180.0 && b < 60.0 {
                    warm_fire_pixels += 1;
                }
            }
        }

        let total = (samples.max(1)) as f64;
        let mut tags = Vec::new();
        if (f64::from(bright_white_pixels) / total) > 0.30 {
            tags.push("snow".to_owned());
        }
        if (f64::from(dark_pixels) / total) > 0.55 {
            tags.push("night".to_owned());
        }
        if (f64::from(green_pixels) / total) > 0.25 {
            tags.push("forest".to_owned());
        }
        if (f64::from(blue_pixels) / total) > 0.30 {
            tags.push("ocean".to_owned());
        }
        if (f64::from(warm_fire_pixels) / total) > 0.20 {
            tags.push("fire".to_owned());
        }

        let face_ratio = f64::from(skin_tone_pixels) / total;
        let mut face_count = if face_ratio > 0.12 {
            tags.push("face".to_owned());
            ((face_ratio * 6.0).round() as u32).clamp(1, 4)
        } else {
            0
        };

        // Execute hardware inference on Hailo-8 / 8L / 10H NPU if available
        let npu_result = self.runtime.infer_image(img);
        let mut backend = "cpu-heuristic".to_owned();
        if let Some(ref npu) = npu_result {
            backend = npu.backend_name.clone();
            let (npu_tags, npu_persons) = map_detections_to_tags_and_faces(&npu.detections);
            for t in npu_tags {
                if !tags.iter().any(|existing| existing == &t) {
                    tags.push(t);
                }
            }
            if npu_persons > 0 {
                face_count = npu_persons;
            }
        }

        // Optional Hailo-10H Vision-Language Model (`hailort::genai::VLM`: Qwen3-VL / Qwen2-VL)
        // when a VLM `.hef` is installed and explicitly enabled (`MBL_HAILO_ENABLE_VLM_INDEXING=1`)
        // or when no fast `InferModel` vision session was available.
        let use_vlm = npu_result.is_none()
            || std::env::var("MBL_HAILO_ENABLE_VLM_INDEXING")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
        let vlm_caption_opt = if use_vlm {
            self.runtime.describe_image_vlm(
                img,
                "Describe this video scene concisely in one sentence, naming key objects, setting, and mood.",
                64,
            )
        } else {
            None
        };
        if let Some((ref vlm_text, ref vlm_backend)) = vlm_caption_opt {
            backend = vlm_backend.clone();
            let lower_vlm = vlm_text.to_ascii_lowercase();
            for (concept, synonyms) in CONCEPTS {
                if (lower_vlm.contains(concept) || synonyms.iter().any(|w| lower_vlm.contains(w)))
                    && !tags.iter().any(|t| t == concept)
                {
                    tags.push((*concept).to_owned());
                }
            }
        }

        // Incorporate semantic hint text (e.g. chapter title, subtitle cue, or filename context)
        if let Some(hint) = hint_text {
            for (concept, synonyms) in CONCEPTS {
                let lower = hint.to_ascii_lowercase();
                if synonyms.iter().any(|w| lower.contains(w)) && !tags.iter().any(|t| t == concept)
                {
                    tags.push((*concept).to_owned());
                }
            }
        }

        if tags.is_empty() {
            tags.push("indoor".to_owned());
        }

        let mut vec = vec![0.0f32; EMBEDDING_DIM];
        // Anchor concept dimensions 0..CONCEPTS.len()
        for (idx, (concept, _)) in CONCEPTS.iter().enumerate() {
            if tags.iter().any(|t| t == concept) {
                vec[idx] = 2.5;
            }
        }
        if let Some(hint) = hint_text {
            let text_emb = embed_text_query(hint);
            for i in 0..EMBEDDING_DIM {
                vec[i] += text_emb[i] * 2.0;
            }
        }
        if let Some((ref vlm_text, _)) = vlm_caption_opt {
            let vlm_emb = embed_text_query(vlm_text);
            for i in 0..EMBEDDING_DIM {
                vec[i] += vlm_emb[i] * 1.5;
            }
        }
        // Incorporate NPU detection confidence or dense embedding energy in dimensions 27..64
        if let Some(ref npu) = npu_result {
            for det in npu.detections.iter().take(12) {
                let slot = 27 + (det.class_id % 36);
                if slot < EMBEDDING_DIM {
                    vec[slot] += det.score * 0.15;
                }
            }
            if !npu.dense_features.is_empty() {
                for (i, &val) in npu.dense_features.iter().take(36).enumerate() {
                    if 27 + i < EMBEDDING_DIM && val.is_finite() {
                        vec[27 + i] += val.clamp(-2.0, 2.0) * 0.05;
                    }
                }
            }
        }
        // Visual color & spatial energy features in dimensions 24..64 (scaled so concept anchors dominate)
        vec[24] = ((r_sum / (total * 255.0)) as f32) * 0.25;
        vec[25] = ((g_sum / (total * 255.0)) as f32) * 0.25;
        vec[26] = ((b_sum / (total * 255.0)) as f32) * 0.25;
        for (i, &val) in spatial_bins.iter().enumerate() {
            if 27 + i < EMBEDDING_DIM {
                vec[27 + i] += (val / ((samples.max(1) as f32) / 32.0)) * 0.05;
            }
        }
        l2_normalize(&mut vec);

        let caption = if let Some((vlm_text, _)) = vlm_caption_opt {
            format!("{} ({})", vlm_text.trim(), tags.join(", "))
        } else {
            match hint_text {
                Some(h) if !h.trim().is_empty() => format!("{} ({})", h.trim(), tags.join(", ")),
                _ => format!("Scene with {}", tags.join(", ")),
            }
        };

        FrameAnalysis { tags, face_count, caption, embedding: vec, backend }
    }

    /// Run a benchmark of semantic frame embedding extraction on the current host (M5.13 Accept).
    pub fn benchmark_frame_embeddings(&self, iterations: usize) -> HailoEmbeddingBenchmark {
        let iterations = iterations.max(1);
        let status = self.runtime_status();
        let pacer = DutyCyclePacer::new(&self.config);
        let img = DynamicImage::ImageRgb8(image::ImageBuffer::from_fn(320, 180, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, 220u8])
        }));

        let started = Instant::now();
        let mut last_backend = self.active_backend_name();
        for i in 0..iterations {
            let batch_start = Instant::now();
            let hint = if i % 2 == 0 {
                Some("car chase in the snow on a mountain road")
            } else {
                Some("night city skyline with rain")
            };
            let res = std::hint::black_box(self.analyze_image(&img, hint));
            last_backend = res.backend;
            let _ = std::hint::black_box(pacer.sleep_after_batch(batch_start.elapsed()));
        }
        let elapsed = started.elapsed();
        let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
        let frames_per_sec = (iterations as f64) / elapsed.as_secs_f64().max(1e-6);
        let avg_latency_us = (elapsed.as_secs_f64() * 1_000_000.0) / (iterations as f64);

        let fp = detect_fingerprint_from_root(Path::new("/"), std::env::consts::ARCH, None);
        let device_nodes = fp.hailo_devices.iter().map(|d| d.device_node.clone()).collect();

        HailoEmbeddingBenchmark {
            backend: last_backend,
            npu_profile: status.npu_profile,
            device_nodes,
            sharing_mode: format!("{:?}", status.sharing_mode),
            vdevice_group_id: status.vdevice_group_id,
            active_co_tenants: status.active_co_tenants.len(),
            iterations,
            elapsed_ms,
            frames_per_sec,
            avg_latency_us,
        }
    }

    /// Generate a strictly spoiler-free "Previously On..." recap from the user's watched episodes (`M5.13`).
    ///
    /// Any episode with `watched == false` is stripped before prompt construction so unwatched plot points
    /// can never leak into the recap.
    pub async fn generate_spoiler_free_recap(
        &self,
        show_title: &str,
        episodes: &[WatchedEpisodeContext],
    ) -> SpoilerFreeRecap {
        let watched: Vec<&WatchedEpisodeContext> = episodes.iter().filter(|e| e.watched).collect();
        let excluded = episodes.len().saturating_sub(watched.len());

        if watched.is_empty() {
            return SpoilerFreeRecap {
                show_title: show_title.to_owned(),
                episodes_included: 0,
                spoiler_protected_unwatched_excluded: excluded,
                recap_text: format!(
                    "No watched episodes recorded yet for {show_title}. Start watching Season 1 to unlock spoiler-free recaps!"
                ),
                model_backend: "none".into(),
            };
        }

        // 1. Try local hailo-ollama first with a short timeout.
        if let Some((llm_recap, backend_label)) =
            self.try_hailo_ollama_recap(show_title, &watched).await
        {
            return SpoilerFreeRecap {
                show_title: show_title.to_owned(),
                episodes_included: watched.len(),
                spoiler_protected_unwatched_excluded: excluded,
                recap_text: llm_recap,
                model_backend: backend_label,
            };
        }

        // 2. Try direct C++ HailoRT `hailort::genai::LLM` on Hailo-10H (works even if hailo-ollama is stopped
        //    or when using C++-only models from Hailo Model Zoo GenAI).
        let mut context_lines = String::new();
        for ep in watched.iter().rev().take(6).rev() {
            context_lines.push_str(&format!(
                "- S{:02}E{:02} {}: {}\n",
                ep.season, ep.episode, ep.title, ep.summary
            ));
        }
        let native_prompt = format!(
            "Summarize the story so far for '{show_title}' in 3 concise sentences using ONLY these watched episodes:\n{context_lines}"
        );
        if let Some((native_recap, native_backend)) =
            self.runtime.generate_text_llm(&native_prompt, 180)
        {
            return SpoilerFreeRecap {
                show_title: show_title.to_owned(),
                episodes_included: watched.len(),
                spoiler_protected_unwatched_excluded: excluded,
                recap_text: native_recap,
                model_backend: native_backend,
            };
        }

        let mut bullets = Vec::with_capacity(watched.len());
        for ep in watched.iter().rev().take(5).rev() {
            let clean_summary = ep.summary.trim();
            if clean_summary.is_empty() {
                bullets.push(format!("S{:02}E{:02} \"{}\"", ep.season, ep.episode, ep.title));
            } else {
                bullets.push(format!(
                    "S{:02}E{:02} ({}): {}",
                    ep.season, ep.episode, ep.title, clean_summary
                ));
            }
        }

        SpoilerFreeRecap {
            show_title: show_title.to_owned(),
            episodes_included: watched.len(),
            spoiler_protected_unwatched_excluded: excluded,
            recap_text: format!(
                "Previously on {show_title} (covering {} watched episode(s); {} unwatched episode(s) excluded for spoiler protection): {}",
                watched.len(),
                excluded,
                bullets.join(" -> ")
            ),
            model_backend: "deterministic-summary".into(),
        }
    }

    async fn try_hailo_ollama_recap(
        &self,
        show_title: &str,
        watched: &[&WatchedEpisodeContext],
    ) -> Option<(String, String)> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(2500))
            .build()
            .ok()?;
        let mut context_lines = String::new();
        for ep in watched.iter().rev().take(6).rev() {
            context_lines.push_str(&format!(
                "- S{:02}E{:02} {}: {}\n",
                ep.season, ep.episode, ep.title, ep.summary
            ));
        }
        let prompt = format!(
            "Summarize the story so far for '{show_title}' in 3 concise sentences using ONLY these watched episodes:\n{context_lines}"
        );

        let primary = self.ollama_url.trim_end_matches('/');
        let mut endpoints = vec![primary.to_owned()];
        if primary == "http://127.0.0.1:8000" {
            endpoints.push("http://127.0.0.1:11434".to_owned());
        }

        for base_url in endpoints {
            // Query `/api/tags` to discover which local model is actually downloaded
            // (Hailo Model Zoo GenAI v5.5.0: qwen3:1.7b, qwen2.5:1.5b, gemma4:e2b, llama3.2:1b, deepseek_r1:1.5b, qwen3:4b)
            let tags_url = format!("{base_url}/api/tags");
            let mut candidate_model = "qwen2.5:1.5b".to_owned();
            if let Ok(tags_resp) = client.get(&tags_url).send().await
                && tags_resp.status().is_success()
                && let Ok(tags_json) = tags_resp.json::<serde_json::Value>().await
                && let Some(models) = tags_json.get("models").and_then(|m| m.as_array())
            {
                let names: Vec<&str> = models
                    .iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()))
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .collect();
                if names.is_empty() {
                    // Server is up, but 0 models are pulled on this endpoint
                    continue;
                }
                const PREFERRED: &[&str] = &[
                    "qwen3:1.7b",
                    "qwen2.5:1.5b",
                    "gemma4:e2b",
                    "llama3.2:1b",
                    "deepseek_r1:1.5b",
                    "qwen3:4b",
                    "qwen2:1.5b",
                ];
                if let Some(&pref) =
                    PREFERRED.iter().find(|&&p| names.iter().any(|n| n.eq_ignore_ascii_case(p)))
                {
                    candidate_model = pref.to_owned();
                } else {
                    candidate_model = names[0].to_owned();
                }
            }

            let url = format!("{base_url}/api/generate");
            let Ok(resp) = client
                .post(&url)
                .json(&serde_json::json!({
                    "model": candidate_model,
                    "prompt": prompt,
                    "stream": false
                }))
                .send()
                .await
            else {
                continue;
            };
            if !resp.status().is_success() {
                continue;
            }
            let Ok(body) = resp.json::<serde_json::Value>().await else {
                continue;
            };
            if let Some(text) = body
                .get("response")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
            {
                return Some((text, format!("hailo-ollama({candidate_model})")));
            }
        }
        None
    }

    /// Transcribe or translate 16 kHz mono float32 PCM audio on Hailo-10H using
    /// `hailort::genai::Speech2Text` (`Whisper-Base.hef`, `Whisper-Small.hef`, `Whisper-Tiny.hef`,
    /// or `Whisper-Medium.hef`).
    pub fn transcribe_audio(
        &self,
        pcm_f32_16k: &[f32],
        language: Option<&str>,
        translate_to_english: bool,
    ) -> Option<(Vec<runtime::HailoSpeechSegment>, String)> {
        self.runtime.transcribe_audio_whisper(pcm_f32_16k, language, translate_to_english)
    }

    /// Describe an image directly on Hailo-10H using `hailort::genai::VLM`
    /// (`Qwen3-VL-2B-Instruct.hef`, `Qwen2-VL-2B-Instruct.hef`, or `Qwen3-VL-4B-Instruct.hef`).
    pub fn describe_image_vlm(
        &self,
        img: &DynamicImage,
        prompt: &str,
        max_tokens: u32,
    ) -> Option<(String, String)> {
        self.runtime.describe_image_vlm(img, prompt, max_tokens)
    }
}

/// Map a natural-language query (e.g. *"find the scene with a car chase in the snow"*) into the
/// 64-dim L2-normalized multimodal embedding space (`M5.13`).
pub fn embed_text_query(query: &str) -> Vec<f32> {
    let mut vec = vec![0.0f32; EMBEDDING_DIM];
    let lower = query.to_ascii_lowercase();
    let tokens: Vec<&str> =
        lower.split(|c: char| !c.is_ascii_alphanumeric()).filter(|s| !s.is_empty()).collect();

    for (idx, (concept, synonyms)) in CONCEPTS.iter().enumerate() {
        for &syn in *synonyms {
            if tokens.contains(&syn) || lower.contains(syn) {
                vec[idx] += 2.5;
            }
        }
        if lower.contains(concept) {
            vec[idx] += 2.5;
        }
    }

    // Token trigram hashing in dimensions 16..24 for open-vocabulary lexical recall
    const STOPWORDS: &[&str] =
        &["find", "the", "scene", "with", "from", "that", "this", "into", "across"];
    for &tok in &tokens {
        if tok.len() >= 4 && !STOPWORDS.contains(&tok) {
            for win in tok.as_bytes().windows(3) {
                let h = ((usize::from(win[0]) * 31 + usize::from(win[1])) * 31
                    + usize::from(win[2]))
                    % 8;
                vec[16 + h] += 0.08;
            }
        }
    }

    // Boost expected visual brightness/color priors for well-known natural-language scene terms
    if vec[0] > 0.0 {
        // snow -> high RGB brightness
        vec[24] += 0.25;
        vec[25] += 0.25;
        vec[26] += 0.28;
    }
    if vec[2] > 0.0 {
        // night -> low brightness
        vec[24] -= 0.15;
        vec[25] -= 0.15;
        vec[26] -= 0.10;
    }

    l2_normalize(&mut vec);
    vec
}

/// Cosine similarity in `[-1.0, 1.0]` between two L2-normalized embedding vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let mut dot = 0.0f32;
    for i in 0..len {
        dot += a[i] * b[i];
    }
    dot
}

fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-6 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_scene_search_ranks_snow_car_chase_first_m5_13() {
        let engine = HailoEngine::default();
        let snowy_img = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            160,
            90,
            image::Rgb([235, 240, 245]),
        ));
        let night_img = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            160,
            90,
            image::Rgb([15, 18, 25]),
        ));
        let forest_img = DynamicImage::ImageRgb8(image::ImageBuffer::from_pixel(
            160,
            90,
            image::Rgb([40, 140, 50]),
        ));

        let scene_snow_chase =
            engine.analyze_image(&snowy_img, Some("High-speed car chase across snow and ice"));
        let scene_night_city =
            engine.analyze_image(&night_img, Some("Quiet night dialogue in downtown city office"));
        let scene_forest_walk =
            engine.analyze_image(&forest_img, Some("Hiking through green forest trail"));

        let q = embed_text_query("find the scene with a car chase in the snow");
        let sim_snow = cosine_similarity(&q, &scene_snow_chase.embedding);
        let sim_night = cosine_similarity(&q, &scene_night_city.embedding);
        let sim_forest = cosine_similarity(&q, &scene_forest_walk.embedding);

        assert!(
            sim_snow > 0.70 && sim_snow > sim_night && sim_snow > sim_forest,
            "expected snow car chase to rank highest: snow={sim_snow}, night={sim_night}, forest={sim_forest}"
        );
    }

    #[test]
    fn npu_detection_and_frame_embedding_benchmark_m5_13() {
        let engine = HailoEngine::default();
        let bench = engine.benchmark_frame_embeddings(25);
        assert_eq!(bench.iterations, 25);
        assert!(bench.frames_per_sec > 10.0);
        assert_eq!(bench.vdevice_group_id, "SHARED");
    }

    #[tokio::test]
    async fn spoiler_free_recap_strictly_excludes_unwatched_episodes_m5_13() {
        let engine = HailoEngine::default().with_ollama_url("http://127.0.0.1:9"); // offline port -> deterministic fallback
        let episodes = vec![
            WatchedEpisodeContext {
                season: 1,
                episode: 1,
                title: "Pilot".into(),
                summary: "The crew discovers the signal on Europa.".into(),
                watched: true,
            },
            WatchedEpisodeContext {
                season: 1,
                episode: 2,
                title: "The Ice Core".into(),
                summary: "Dr. Chen decodes the beacon coordinates.".into(),
                watched: true,
            },
            WatchedEpisodeContext {
                season: 1,
                episode: 3,
                title: "Betrayal".into(),
                summary: "SECRET SPOILER: Commander Vance is the saboteur!".into(),
                watched: false,
            },
        ];

        let recap = engine.generate_spoiler_free_recap("Europa Nine", &episodes).await;
        assert_eq!(recap.episodes_included, 2);
        assert_eq!(recap.spoiler_protected_unwatched_excluded, 1);
        assert!(recap.recap_text.contains("Europa"));
        assert!(!recap.recap_text.contains("SECRET SPOILER"));
        assert!(!recap.recap_text.contains("Vance"));
    }

    #[test]
    fn parses_hailo_nms_by_class_and_maps_semantic_tags() {
        let mut raw = vec![0.0f32; 80 + 10];
        // Class 0 ("person"): 1 detection at score 0.92
        raw[0] = 1.0;
        raw[1] = 0.10;
        raw[2] = 0.20;
        raw[3] = 0.80;
        raw[4] = 0.60;
        raw[5] = 0.92;
        // Class 1 ("bicycle"): 0 detections
        raw[6] = 0.0;
        // Class 2 ("car"): 1 detection at score 0.85
        raw[7] = 1.0;
        raw[8] = 0.40;
        raw[9] = 0.10;
        raw[10] = 0.90;
        raw[11] = 0.90;
        raw[12] = 0.85;

        let dets = runtime::parse_nms_by_class(&raw, 80, 0.25);
        assert_eq!(dets.len(), 2);
        assert_eq!(dets[0].label, "person");
        assert!((dets[0].score - 0.92).abs() < 1e-4);
        assert_eq!(dets[1].label, "car");

        let (tags, faces) = runtime::map_detections_to_tags_and_faces(&dets);
        assert_eq!(faces, 1);
        assert!(tags.contains(&"face".to_owned()));
        assert!(tags.contains(&"person".to_owned()));
        assert!(tags.contains(&"car_chase".to_owned()));
        assert!(tags.contains(&"car".to_owned()));
    }

    #[test]
    fn live_hailo_npu_hardware_probe_and_real_image_inference() {
        let fp = detect_fingerprint_from_root(Path::new("/"), std::env::consts::ARCH, None);
        if !fp.npu_profile.is_present() {
            return;
        }
        let probe = runtime::HailoNativeRuntime::probe_device("SHARED");
        assert!(probe.is_some(), "expected libhailort.so device probe to succeed on Hailo host");
        let probe = probe.unwrap();
        assert!(!probe.firmware_version.is_empty());
        assert!(probe.architecture.starts_with("HAILO"));

        let engine = HailoEngine::default();
        if mbl_hwaccel::hailo::discover_hef_model_for_profile(Path::new("/"), fp.npu_profile)
            .is_some()
        {
            assert!(
                engine.active_backend_name().starts_with("hailort-npu("),
                "expected active_backend_name to be hailort-npu(...), got {}",
                engine.active_backend_name()
            );
            let photo_path = Path::new("/home/marcus/philip1.png");
            if photo_path.exists()
                && let Ok(img) = image::open(photo_path)
            {
                let analysis = engine.analyze_image(&img, None);
                assert!(analysis.backend.starts_with("hailort-npu("));
                assert!(
                    analysis.face_count >= 1 && analysis.tags.contains(&"person".to_owned()),
                    "expected Hailo NPU to detect person in philip1.png: tags={:?}, faces={}",
                    analysis.tags,
                    analysis.face_count
                );
            }
        }
    }
}
