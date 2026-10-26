//! Media Intelligence Features (M7.18):
//! - Scene X-Ray on Pause (cast & soundtrack in current scene)
//! - Spoiler-Free "Previously On..." AI Recap (strictly bounded by user's watch history)
//! - Binge Sleep Guard (auto-pause and bookmark last awake episode)
//! - Shareable Clip & GIF Creator (5-30s slice with optional burned-in subtitles)

use mbl_core::ItemId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

// -----------------------------------------------------------------------------
// 1. Scene X-Ray
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneActor {
    pub name: String,
    pub character: Option<String>,
    pub thumb_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneSong {
    pub title: String,
    pub artist: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneXRay {
    pub current_time_ms: i64,
    pub actors: Vec<SceneActor>,
    pub soundtrack: Option<SceneSong>,
}

// -----------------------------------------------------------------------------
// 2. Spoiler-Free Recap Builder
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeContext {
    pub episode_id: ItemId,
    pub season_number: i64,
    pub episode_number: i64,
    pub title: String,
    pub summary: Option<String>,
    pub transcript_cue_texts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpoilerFreeRecapPrompt {
    pub show_title: String,
    pub target_episode_id: ItemId,
    pub eligible_context_episodes: Vec<EpisodeContext>,
    pub prompt_text: String,
}

/// Builds recap context strictly bounded by user's watched episode IDs.
/// Unwatched episodes are unconditionally excluded to guarantee zero spoilers.
pub fn build_spoiler_free_recap(
    show_title: &str,
    target_episode_id: ItemId,
    all_show_episodes: &[EpisodeContext],
    watched_episode_ids: &HashSet<ItemId>,
) -> SpoilerFreeRecapPrompt {
    let mut eligible = Vec::new();
    for ep in all_show_episodes {
        // Exclude target episode itself and any unwatched episodes
        if ep.episode_id != target_episode_id && watched_episode_ids.contains(&ep.episode_id) {
            eligible.push(ep.clone());
        }
    }

    let mut context_summary = String::new();
    for ep in &eligible {
        context_summary.push_str(&format!(
            "Season {} Episode {}: {}\n",
            ep.season_number, ep.episode_number, ep.title
        ));
        if let Some(ref s) = ep.summary {
            context_summary.push_str(&format!("Summary: {}\n", s));
        }
    }

    let prompt_text = format!(
        "You are an AI companion providing a spoiler-free recap of previous events for '{}'.\n\
         IMPORTANT: Only summarize the events described in the following watched episodes. Do NOT mention or speculate about any future events:\n\n\
         {}\n\
         Provide a concise, dramatic 2-3 paragraph 'Previously on...' recap leading up to the current episode.",
        show_title, context_summary
    );

    SpoilerFreeRecapPrompt {
        show_title: show_title.to_owned(),
        target_episode_id,
        eligible_context_episodes: eligible,
        prompt_text,
    }
}

// -----------------------------------------------------------------------------
// 3. Binge Sleep Guard
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SleepGuardConfig {
    pub max_consecutive_episodes: u32,
}

impl Default for SleepGuardConfig {
    fn default() -> Self {
        Self { max_consecutive_episodes: 3 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepGuardDecision {
    pub should_pause: bool,
    pub consecutive_count: u32,
    pub bookmarked_episode_id: Option<ItemId>,
}

/// Evaluates if playback should be paused due to user inactivity across consecutive episodes.
pub fn check_sleep_guard(
    consecutive_autoplay_count: u32,
    has_recent_interaction: bool,
    last_confirmed_awake_episode_id: Option<ItemId>,
    config: SleepGuardConfig,
) -> SleepGuardDecision {
    if !has_recent_interaction && consecutive_autoplay_count >= config.max_consecutive_episodes {
        SleepGuardDecision {
            should_pause: true,
            consecutive_count: consecutive_autoplay_count,
            bookmarked_episode_id: last_confirmed_awake_episode_id,
        }
    } else {
        SleepGuardDecision {
            should_pause: false,
            consecutive_count: consecutive_autoplay_count,
            bookmarked_episode_id: None,
        }
    }
}

// -----------------------------------------------------------------------------
// 4. Shareable Clip Creator
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipFormat {
    Mp4,
    Webm,
    Gif,
}

pub fn build_clip_ffmpeg_args(
    input_path: &Path,
    start_secs: f64,
    duration_secs: f64,
    format: ClipFormat,
    burn_subtitle_path: Option<&Path>,
    output_path: &Path,
) -> Vec<String> {
    let mut args = vec![
        "-nostdin".into(),
        "-hide_banner".into(),
        "-ss".into(),
        format!("{:.3}", start_secs),
        "-t".into(),
        format!("{:.3}", duration_secs),
        "-i".into(),
        input_path.to_string_lossy().into_owned(),
    ];

    match format {
        ClipFormat::Mp4 => {
            if let Some(sub) = burn_subtitle_path {
                args.extend([
                    "-vf".into(),
                    format!("subtitles='{}'", sub.to_string_lossy().replace('\'', "'\\''")),
                ]);
            }
            args.extend([
                "-c:v".into(),
                "libx264".into(),
                "-preset".into(),
                "fast".into(),
                "-crf".into(),
                "22".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "128k".into(),
                "-movflags".into(),
                "+faststart".into(),
            ]);
        }
        ClipFormat::Webm => {
            args.extend([
                "-c:v".into(),
                "libvpx-vp9".into(),
                "-crf".into(),
                "30".into(),
                "-b:v".into(),
                "0".into(),
                "-c:a".into(),
                "libopus".into(),
            ]);
        }
        ClipFormat::Gif => {
            args.extend([
                "-vf".into(),
                "fps=15,scale=480:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse".into(),
            ]);
        }
    }

    args.extend(["-y".into(), output_path.to_string_lossy().into_owned()]);
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recap_context_strictly_excludes_unwatched_episodes() {
        let ep1 = EpisodeContext {
            episode_id: ItemId(101),
            season_number: 1,
            episode_number: 1,
            title: "Pilot".into(),
            summary: Some("Intro to world".into()),
            transcript_cue_texts: vec![],
        };
        let ep2 = EpisodeContext {
            episode_id: ItemId(102),
            season_number: 1,
            episode_number: 2,
            title: "The Twist".into(),
            summary: Some("Major twist occurs".into()),
            transcript_cue_texts: vec![],
        };
        let ep3 = EpisodeContext {
            episode_id: ItemId(103),
            season_number: 1,
            episode_number: 3,
            title: "The Aftermath".into(),
            summary: Some("Spoilers ahead".into()),
            transcript_cue_texts: vec![],
        };

        let mut watched = HashSet::new();
        watched.insert(ItemId(101)); // Only ep1 watched

        let recap =
            build_spoiler_free_recap("Mystery Show", ItemId(102), &[ep1, ep2, ep3], &watched);

        assert_eq!(recap.eligible_context_episodes.len(), 1);
        assert_eq!(recap.eligible_context_episodes[0].episode_id, ItemId(101));
        assert!(recap.prompt_text.contains("Pilot"));
        assert!(!recap.prompt_text.contains("The Twist"));
        assert!(!recap.prompt_text.contains("The Aftermath"));
    }

    #[test]
    fn sleep_guard_triggers_after_threshold() {
        let config = SleepGuardConfig { max_consecutive_episodes: 3 };

        let decision_active = check_sleep_guard(2, false, Some(ItemId(5)), config);
        assert!(!decision_active.should_pause);

        let decision_asleep = check_sleep_guard(3, false, Some(ItemId(5)), config);
        assert!(decision_asleep.should_pause);
        assert_eq!(decision_asleep.bookmarked_episode_id, Some(ItemId(5)));
    }
}
