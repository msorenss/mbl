//! Persistent DVR reservations executed by owned, cancellable FFmpeg children.
use mbl_core::{RecordingId, time::now_ms};
use mbl_db::{Db, repo::m9};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub async fn run(
    db: Db,
    livetv: mbl_service::LiveTvService,
    ffmpeg: PathBuf,
    output: PathBuf,
    stop: CancellationToken,
) {
    if let Err(error) = m9::recover_interrupted_recordings(&db, now_ms()).await {
        tracing::error!(%error, "DVR recovery failed");
        return;
    }
    if let Err(error) = tokio::fs::create_dir_all(&output).await {
        tracing::error!(%error, "DVR output directory unavailable");
        return;
    }
    let mut tasks = JoinSet::new();
    // Recover old media in parallel with scheduling: a large archive must not
    // delay a due recording until its broadcast window has passed.
    let recovery = livetv.clone();
    let recovery_stop = stop.clone();
    tasks.spawn(async move {
        tokio::select! {
            _ = recovery_stop.cancelled() => {},
            _ = recovery.reconcile_recordings() => {},
        }
    });
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
    loop {
        tokio::select! {
            _ = stop.cancelled() => break,
            Some(result) = tasks.join_next(), if !tasks.is_empty() => { if let Err(error) = result { tracing::error!(%error, "DVR task failed"); } },
            _ = tick.tick() => {
                match m9::list_recordings(&db, Some("scheduled")).await {
                    Ok(rows) => for row in rows {
                        let now = now_ms();
                        if row.end_time <= now {
                            let _ = m9::update_recording_status(&db, RecordingId(row.id), "failed", None, None, None, Some("Recording window missed")).await;
                        } else if row.start_time <= now && tasks.len() < 32 {
                            match m9::claim_recording(&db, RecordingId(row.id), now).await {
                                Ok(true) => { tasks.spawn(record(db.clone(), livetv.clone(), ffmpeg.clone(), output.clone(), row, stop.clone())); },
                                Ok(false) => {}, Err(error) => tracing::error!(%error, "DVR claim failed"),
                            }
                        }
                    },
                    Err(error) => tracing::error!(%error, "DVR schedule read failed"),
                }
            }
        }
    }
    // Each child observes stop, kills and reaps FFmpeg, and saves the terminal status.
    while tasks.join_next().await.is_some() {}
}

async fn record(
    db: Db,
    livetv: mbl_service::LiveTvService,
    ffmpeg: PathBuf,
    output: PathBuf,
    row: m9::RecordingRow,
    stop: CancellationToken,
) {
    let id = RecordingId(row.id);
    let path = output.join(format!("{}.ts", row.id));
    let mut cancelled = false;
    let mut tuner_lease = None;
    let result = async {
        tuner_lease = Some(livetv.reserve_recording(id, mbl_core::ChannelId(row.channel_id), row.end_time).await?);
        let channel = m9::get_channel(&db, mbl_core::ChannelId(row.channel_id)).await?.ok_or(mbl_core::Error::NotFound)?;
        if !channel.stream_url.starts_with("http://") && !channel.stream_url.starts_with("https://") {
            return Err(mbl_core::Error::Validation("DVR requires an HTTP or HTTPS channel stream".into()));
        }
        let remaining = ((row.end_time - now_ms()).max(1) as f64 / 1000.0).to_string();
        let mut child = Command::new(ffmpeg).args(["-nostdin", "-hide_banner", "-loglevel", "error", "-n", "-protocol_whitelist", "http,https,tcp,tls,crypto", "-rw_timeout", "15000000", "-i", &channel.stream_url, "-t", &remaining, "-map", "0:v?", "-map", "0:a?", "-c", "copy", "-f", "mpegts"])
            .arg(&path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).spawn().map_err(mbl_core::Error::internal)?;
        let mut check = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                status = child.wait() => { let status = status.map_err(mbl_core::Error::internal)?; if status.success() { break; } return Err(mbl_core::Error::External(format!("FFmpeg exited with {status}"))); },
                _ = stop.cancelled() => { let _ = child.kill().await; return Err(mbl_core::Error::Unavailable("Recording interrupted by server shutdown".into())); },
                _ = check.tick() => {
                    let recording = m9::get_recording(&db, id).await?;
                    if recording.is_none_or(|r| r.status != "recording") { let _ = child.kill().await; let _ = child.wait().await; cancelled = true; return Ok(()); }
                    if now_ms() > row.end_time.saturating_add(15_000) { let _ = child.kill().await; return Err(mbl_core::Error::External("Recording exceeded its deadline".into())); }
                }
            }
        }
        Ok(())
    }.await;
    if let Some(lease) = tuner_lease {
        lease.release().await;
    }
    if cancelled || matches!(m9::get_recording(&db, id).await, Ok(None)) {
        let _ = tokio::fs::remove_file(&path).await;
        return;
    }
    let bytes = tokio::fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0);
    let result = match result {
        Ok(()) if bytes > 0 => tokio::select! {
            _ = stop.cancelled() => Err(mbl_core::Error::Unavailable("Recording publication interrupted".into())),
            result = livetv.publish_recording(id) => result.map(|_| ()),
        },
        Ok(()) => Err(mbl_core::Error::Validation("Recording produced no media".into())),
        Err(error) => Err(error),
    };
    if result.is_ok() {
        return;
    } // Publication saves status and probed duration in one transaction.
    // A deletion racing with finalization must not leave either a file or catalogue item.
    if matches!(m9::get_recording(&db, id).await, Ok(None)) {
        let _ = tokio::fs::remove_file(&path).await;
        return;
    }
    let error = result
        .err()
        .map(|e| e.to_string())
        .or_else(|| (bytes == 0).then(|| "Recording produced no media".into()));
    if let Err(error) =
        m9::finish_recording(&db, id, false, &path.to_string_lossy(), bytes, 0, error.as_deref())
            .await
    {
        tracing::error!(%error, "DVR final status write failed");
    }
}
