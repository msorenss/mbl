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
    playback: mbl_service::PlaybackService,
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
    let mut background = JoinSet::new();
    // Recover old media in parallel with scheduling: a large archive must not
    // delay a due recording until its broadcast window has passed.
    let recovery = livetv.clone();
    let recovery_stop = stop.clone();
    background.spawn(async move {
        tokio::select! {
            _ = recovery_stop.cancelled() => {},
            _ = recovery.reconcile_recordings() => {},
        }
    });
    let maintenance = livetv.clone();
    let maintenance_stop = stop.clone();
    background.spawn(async move {
        let mut tick=tokio::time::interval(std::time::Duration::from_secs(15));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _=maintenance_stop.cancelled()=>break,
                _=tick.tick()=>{
                    tokio::select! {
                        _=maintenance_stop.cancelled()=>break,
                        result=playback.maintain_dvr(&maintenance)=>if let Err(error)=result {tracing::error!(%error,"DVR maintenance failed");},
                    }
                }
            }
        }
    });
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
    loop {
        tokio::select! {
            _ = stop.cancelled() => break,
            Some(result) = background.join_next(), if !background.is_empty() => { if let Err(error) = result { tracing::error!(%error, "DVR background task failed"); } },
            Some(result) = tasks.join_next(), if !tasks.is_empty() => { if let Err(error) = result { tracing::error!(%error, "DVR task failed"); } },
            _ = tick.tick() => {
                match m9::list_recordings(&db, Some("scheduled")).await {
                    Ok(rows) => for row in rows {
                        let now = now_ms();
                        if row.end_time <= now {
                            let _ = m9::update_recording_status(&db, RecordingId(row.id), "failed", None, None, None, Some("Recording window missed")).await;
                        } else if row.start_time <= now && tasks.len() < 32 {
                            if let Err(error)=livetv.authorize_recording_job(&row).await {
                                let message=format!("Recording no longer authorized or source unavailable: {error}");
                                let _=m9::update_recording_status(&db,RecordingId(row.id),"failed",None,None,None,Some(&message)).await;
                                continue;
                            }
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
    while background.join_next().await.is_some() {}
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
        livetv.authorize_recording_job(&row).await?;
        let byte_limit = tokio::select! {
            _=stop.cancelled()=>return Err(mbl_core::Error::Unavailable("Recording interrupted by server shutdown".into())),
            result=tokio::time::timeout(std::time::Duration::from_secs(2),livetv.recording_storage_limit())=>result.map_err(|_|mbl_core::Error::Unavailable("DVR storage check timed out".into()))??,
        };
        if row.end_time <= now_ms() {return Err(mbl_core::Error::Validation("Recording window ended before capture could start".into()));}
        let channel_id=row.channel_id.ok_or(mbl_core::Error::NotFound)?;
        tuner_lease = Some(livetv.reserve_recording(id, mbl_core::ChannelId(channel_id), row.end_time).await?);
        let channel = m9::get_channel(&db, mbl_core::ChannelId(channel_id)).await?.ok_or(mbl_core::Error::NotFound)?;
        if !channel.stream_url.starts_with("http://") && !channel.stream_url.starts_with("https://") {
            return Err(mbl_core::Error::Validation("DVR requires an HTTP or HTTPS channel stream".into()));
        }
        let remaining = ((row.end_time - now_ms()).max(1) as f64 / 1000.0).to_string();
        let deadline = tokio::time::Instant::now().checked_add(std::time::Duration::from_millis(
            row.end_time.saturating_sub(now_ms()).max(0) as u64,
        )).ok_or_else(|| mbl_core::Error::Validation("Recording end is too far in the future".into()))?;
        let end = tokio::time::sleep_until(deadline);
        tokio::pin!(end);
        let mut child = Command::new(ffmpeg).args(["-nostdin", "-hide_banner", "-loglevel", "error", "-n", "-protocol_whitelist", "http,https,tcp,tls,crypto", "-rw_timeout", "15000000", "-i", &channel.stream_url, "-t", &remaining, "-fs", &byte_limit.to_string(), "-map", "0:v?", "-map", "0:a?", "-map", "0:s?", "-c", "copy", "-f", "mpegts"])
            .arg(&path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true).spawn().map_err(mbl_core::Error::internal)?;
        let mut check = tokio::time::interval(std::time::Duration::from_secs(1));
        let mut last_policy_check=std::time::Instant::now();
        let mut source_failure = None;
        loop {
            tokio::select! {
                status = child.wait() => { let status = status.map_err(mbl_core::Error::internal)?; if !status.success() { source_failure = Some(format!("FFmpeg exited with {status}")); } break; },
                _ = stop.cancelled() => { let _ = child.kill().await; return Err(mbl_core::Error::Unavailable("Recording interrupted by server shutdown".into())); },
                // Media timestamps alone cannot bound a stalled real-time source.
                // Stop at the approved wall-clock end; probing decides completeness.
                _ = &mut end => { let _ = child.kill().await; let _ = child.wait().await; break; },
                _ = check.tick() => {
                    let policy_due=last_policy_check.elapsed()>=std::time::Duration::from_secs(5);
                    let inspected=tokio::select! {
                        _=stop.cancelled()=> {let _=child.kill().await;return Err(mbl_core::Error::Unavailable("Recording interrupted by server shutdown".into()));},
                        _=&mut end=> {let _=child.kill().await;let _=child.wait().await;break;},
                        result=tokio::time::timeout(std::time::Duration::from_secs(2),inspect_capture(&db,&livetv,id,&path,byte_limit,policy_due))=>result.unwrap_or_else(|_|Err(mbl_core::Error::Unavailable("DVR storage/policy check timed out".into()))),
                    };
                    let inspected=match inspected {Ok(action)=>action,Err(error)=>{let _=child.kill().await;let _=child.wait().await;return Err(error);}};
                    match inspected {
                        CaptureCheck::Cancelled=> {let _=child.kill().await;let _=child.wait().await;cancelled=true;return Ok(None);},
                        CaptureCheck::StorageStop(reason)=> {let _=child.kill().await;let _=child.wait().await;source_failure=Some(reason);break;},
                        CaptureCheck::Continue=>{},
                    }
                    if policy_due {last_policy_check=std::time::Instant::now();}
                    if now_ms() >= row.end_time { let _=child.kill().await; let _=child.wait().await; break; }
                }
            }
        }
        if source_failure.is_none() && tokio::fs::metadata(&path).await.is_ok_and(|m|m.len()>=byte_limit) {
            source_failure=Some("DVR recording size limit reached".into());
        }
        Ok(source_failure)
    }.await;
    if let Some(lease) = tuner_lease {
        lease.release().await;
    }
    if cancelled || abandoned(&db, id).await {
        let _ = tokio::fs::remove_file(&path).await;
        return;
    }
    let bytes = tokio::fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0);
    let result = match result {
        Ok(failure) if bytes > 0 => tokio::select! {
            _ = stop.cancelled() => Err(mbl_core::Error::Unavailable("Recording publication interrupted".into())),
            result = livetv.publish_recording_capture(id, failure.as_deref()) => result.map(|_| ()),
        },
        Ok(_) => Err(mbl_core::Error::Validation("Recording produced no media".into())),
        Err(error) => Err(error),
    };
    if result.is_ok() {
        return;
    } // Publication saves status and probed duration in one transaction.
    // A deletion racing with finalization must not leave either a file or catalogue item.
    if abandoned(&db, id).await {
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

enum CaptureCheck {
    Continue,
    Cancelled,
    StorageStop(String),
}
async fn inspect_capture(
    db: &Db,
    livetv: &mbl_service::LiveTvService,
    id: RecordingId,
    path: &std::path::Path,
    limit: u64,
    policy_due: bool,
) -> mbl_core::Result<CaptureCheck> {
    let Some(row) = m9::get_recording(db, id).await?.filter(|r| r.status == "recording") else {
        return Ok(CaptureCheck::Cancelled);
    };
    if policy_due {
        livetv.authorize_recording_job(&row).await?;
    }
    if let Err(error) = livetv.recording_storage_limit().await {
        return Ok(CaptureCheck::StorageStop(error.to_string()));
    }
    if tokio::fs::metadata(path).await.is_ok_and(|m| m.len() >= limit) {
        return Ok(CaptureCheck::StorageStop("DVR recording size limit reached".into()));
    }
    Ok(CaptureCheck::Continue)
}

async fn abandoned(db: &Db, id: RecordingId) -> bool {
    match m9::get_recording(db, id).await {
        Ok(None) => true,
        Ok(Some(row)) => matches!(row.status.as_str(), "failed" | "cancelled"),
        Err(_) => false,
    }
}
