import { useNavigate, useParams } from "@solidjs/router";
import { createEffect, createResource, createSignal, For, lazy, Show, Suspense } from "solid-js";
import client from "../api/client";
import { getDeviceProfileInput } from "../api/deviceProfile";
import type { components } from "../api/schema";
import { collectTracks, shuffled } from "../api/tracks";
import ArrLinkButton from "../components/ArrLinkButton";
import MetadataEditor from "../components/MetadataEditor";
import MissingEpisodes from "../components/MissingEpisodes";
import PlayOnDevice from "../components/PlayOnDevice";
import RecapPanel from "../components/RecapPanel";
import SubtitleOptions from "../components/SubtitleOptions";
import { Badge, Button, Chip } from "../components/ui";
import { useAuth } from "../context/AuthContext";
import { usePlayer } from "../context/PlayerContext";
import { useTheme } from "../context/ThemeContext";
import { useToast } from "../context/ToastContext";
import { useI18n } from "../i18n";
import {
  FREQUENCY_KEY,
  MODE_KEY,
  storedFrequency,
  storedMode,
  storePref,
} from "../utils/companionPrefs";
import { subtitleSearchAvailable } from "../utils/subtitleSearch";

const SubtitleSearch = lazy(() => import("../components/SubtitleSearch"));

type ItemDetailDto = components["schemas"]["ItemDetail"];
type ItemSummary = components["schemas"]["ItemSummary"];
type PlaybackPreview = components["schemas"]["PlaybackPreview"];

export function ItemDetail() {
  const params = useParams();
  const itemId = () => Number(params.id);
  const { playItem, playQueue, addToQueue, playbackRevision } = usePlayer();
  const [queueBusy, setQueueBusy] = createSignal(false);
  const playAll = async (shuffle: boolean) => {
    const current = item();
    if (!current) return;
    setQueueBusy(true);
    try {
      const tracks = await collectTracks(current.id, current.kind);
      if (tracks.length === 0) {
        showToast({ title: "Nothing to play", message: "No tracks were found.", type: "info" });
        return;
      }
      playQueue(shuffle ? shuffled(tracks) : tracks);
    } catch {
      showToast({ title: "Error", message: "Could not load the tracks.", type: "error" });
    } finally {
      setQueueBusy(false);
    }
  };
  const { showToast } = useToast();
  const theme = useTheme();
  const { t } = useI18n();
  const { user } = useAuth();
  const navigate = useNavigate();

  const [item, setItem] = createSignal<ItemDetailDto | null>(null);
  const [children, setChildren] = createSignal<ItemSummary[]>([]);
  const [preview, setPreview] = createSignal<PlaybackPreview | null>(null);
  const [selectedVersionId, setSelectedVersionId] = createSignal<number | null>(null);
  const [selectedAudioIdx, setSelectedAudioIdx] = createSignal<number | null>(null);
  const [selectedSubIdx, setSelectedSubIdx] = createSignal<number | null>(null);
  // The companion choice for the next playback (the player sends it when it starts).
  const [companionChoice, setCompanionChoiceSignal] = createSignal(
    storedMode() === "on" ? `on:${storedFrequency()}` : storedMode(),
  );
  const setCompanionChoice = (value: string) => {
    setCompanionChoiceSignal(value);
    const [mode, frequency] = value.split(":");
    storePref(MODE_KEY, mode);
    if (frequency) storePref(FREQUENCY_KEY, frequency);
  };
  const QUALITY_KEY = "mbl.playback.quality";
  const [selectedQuality, setSelectedQualitySignal] = createSignal<number>(
    (() => {
      try {
        const stored = Number(localStorage.getItem(QUALITY_KEY));
        return Number.isSafeInteger(stored) && stored > 0 ? stored : 0;
      } catch {
        return 0;
      }
    })(),
  );
  const setSelectedQuality = (value: number) => {
    setSelectedQualitySignal(value);
    try {
      localStorage.setItem(QUALITY_KEY, String(value));
    } catch {
      /* The choice still applies to this playback. */
    }
  };
  const [loadError, setLoadError] = createSignal<string | null>(null);
  let loadVersion = 0;
  const [loading, setLoading] = createSignal(true);
  const [watchUpdating, setWatchUpdating] = createSignal(false);
  const [subtitlesAvailable] = createResource(subtitleSearchAvailable);
  const [findingSubtitles, setFindingSubtitles] = createSignal(false);
  // What Play starts on a show or season page.
  const [nextEpisode, setNextEpisode] = createSignal<components["schemas"]["NextEpisode"]>();

  const loadData = async () => {
    const version = ++loadVersion;
    const id = itemId();
    setLoading(true);
    setLoadError(null);
    setItem(null);
    setChildren([]);
    setPreview(null);
    setSelectedVersionId(null);
    setSelectedAudioIdx(null);
    setSelectedSubIdx(null);
    setNextEpisode(undefined);
    try {
      const [detailRes, childrenRes] = await Promise.all([
        client.GET("/api/v1/items/{id}", {
          params: { path: { id } },
        }),
        client.GET("/api/v1/items/{id}/children", {
          params: { path: { id } },
        }),
      ]);

      if (version !== loadVersion) return;
      if (!detailRes.data) throw new Error("Media details could not be loaded.");
      if (detailRes.data) setItem(detailRes.data);
      if (childrenRes.data) setChildren(childrenRes.data);
      if (detailRes.data.kind === "show" || detailRes.data.kind === "season") {
        const next = await client
          .GET("/api/v1/items/{id}/next-episode", { params: { path: { id } } })
          .catch(() => undefined);
        if (version === loadVersion && next?.data) setNextEpisode(next.data);
      }
    } catch {
      if (version === loadVersion) setLoadError("Media details could not be loaded.");
    } finally {
      if (version === loadVersion) setLoading(false);
    }
  };

  createEffect(() => {
    itemId();
    playbackRevision();
    void loadData();
  });

  let previewVersion = 0;
  createEffect(() => {
    const id = itemId(),
      part = selectedVersionId(),
      audio = selectedAudioIdx(),
      subtitle = selectedSubIdx();
    const ready = item();
    const version = ++previewVersion;
    setPreview(null);
    if (!ready?.versions.length) return;
    void client
      .GET("/api/v1/items/{id}/playback-preview", {
        params: {
          path: { id },
          query: {
            profile: JSON.stringify(getDeviceProfileInput()),
            part_id: part ?? undefined,
            audio_stream_index: audio ?? undefined,
            subtitle_stream_index: subtitle !== null && subtitle >= 0 ? subtitle : undefined,
          },
        },
      })
      .then((result) => {
        if (version === previewVersion && result.data) setPreview(result.data);
      })
      .catch(() => {});
  });

  const formatDuration = (ms?: number | null) => {
    if (!ms || ms <= 0) return null;
    const totalSecs = Math.floor(ms / 1000);
    const hours = Math.floor(totalSecs / 3600);
    const mins = Math.floor((totalSecs % 3600) / 60);
    if (hours > 0) return `${hours}h ${mins}m`;
    return `${mins}m`;
  };

  const endsAtTime = (ms?: number | null) => {
    if (!ms || ms <= 0) return null;
    const endTime = new Date(Date.now() + ms);
    return endTime.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  };

  const backdropArt = () => item()?.artwork.find((a) => a.kind === "backdrop");
  const posterArt = () => item()?.artwork.find((a) => a.kind === "poster");

  const backdropUrl = () => {
    const art = backdropArt();
    return art ? `/api/v1/images/${art.id}?w=1280` : null;
  };

  const posterUrl = () => {
    const art = posterArt();
    return art ? `/api/v1/images/${art.id}?w=500` : null;
  };

  // Ambient palette colors from artwork (M3.4)
  const ambientColors = () => {
    const p = backdropArt()?.palette || posterArt()?.palette || [];
    const dominant = p[0] || "#141721";
    const vibrant = p[1] || "var(--accent)";
    return { dominant, vibrant };
  };

  const isWatched = () => (item()?.watch.viewCount ?? 0) > 0;
  const viewOffsetMs = () => item()?.watch.viewOffsetMs ?? 0;

  // Primary video version & streams
  const primaryVersion = () =>
    item()?.versions?.find((version) => version.id === selectedVersionId()) ??
    item()?.versions?.[0];
  const primaryPart = () => primaryVersion()?.parts?.[0];
  const audioStreams = () => primaryPart()?.streams.filter((s) => s.kind === "audio") || [];
  const subStreams = () => primaryPart()?.streams.filter((s) => s.kind === "subtitle") || [];

  const isSeries = () => item()?.kind === "show" || item()?.kind === "season";
  /** "S2E9" for the episode Play starts on a show or season page. */
  const nextCode = () => {
    const ep = nextEpisode()?.episode;
    if (!ep) return "";
    return `S${ep.parentIdx ?? 1}E${ep.idx ?? 1}`;
  };
  const playable = () => (isSeries() ? Boolean(nextEpisode()) : Boolean(primaryPart()));
  const resumeAt = () => (isSeries() ? (nextEpisode()?.startMs ?? 0) : Math.max(0, viewOffsetMs()));

  const handlePlay = (startOffset = 0, castFirst = false) => {
    const it = item();
    if (!it) return;
    const next = nextEpisode();
    if (isSeries()) {
      if (!next) return;
      playItem({
        itemId: next.episode.id,
        initialStartMs: startOffset,
        maxBitrate: selectedQuality() || undefined,
        castFirst,
        autoplayNext: true,
        title: next.episode.title,
        subtitle: `${it.title} · ${nextCode()}`,
      });
      return;
    }
    playItem({
      itemId: it.id,
      partId:
        selectedVersionId() !== null || selectedAudioIdx() !== null || selectedSubIdx() !== null
          ? primaryPart()?.id
          : undefined,
      audioStreamIndex: selectedAudioIdx() ?? undefined,
      subtitleStreamIndex: selectedSubIdx(),
      initialStartMs: startOffset,
      maxBitrate: selectedQuality() || undefined,
      castFirst,
      title: it.title,
      subtitle: it.originallyAvailableAt ? it.originallyAvailableAt.substring(0, 4) : undefined,
    });
  };

  const handleToggleWatched = async () => {
    if (watchUpdating()) return;
    setWatchUpdating(true);

    try {
      if (isWatched()) {
        const result = await client.DELETE("/api/v1/items/{id}/watched", {
          params: { path: { id: itemId() } },
        });
        if (result.error) throw new Error("Could not update watch state.");
        showToast({ title: "Marked Unwatched", message: item()?.title || "", type: "info" });
      } else {
        const result = await client.POST("/api/v1/items/{id}/watched", {
          params: { path: { id: itemId() } },
        });
        if (result.error) throw new Error("Could not update watch state.");
        showToast({ title: "Marked Watched", message: item()?.title || "", type: "success" });
      }
      await loadData();
    } catch (e) {
      showToast({
        title: "Action Failed",
        message: e instanceof Error ? e.message : "Error updating watch state",
        type: "danger",
      });
    } finally {
      setWatchUpdating(false);
    }
  };

  return (
    <Show
      when={!loading() && item()}
      fallback={
        <div style={{ padding: "60px 0", "text-align": "center", color: "var(--text-muted)" }}>
          {loadError() || "Loading media details..."}
        </div>
      }
    >
      <div
        style={{
          position: "relative",
          "border-radius": "var(--radius-lg)",
          overflow: "hidden",
          background: `radial-gradient(circle at 80% 20%, ${ambientColors().vibrant}33 0%, transparent 60%), radial-gradient(circle at 20% 80%, ${ambientColors().dominant}88 0%, var(--bg-base) 100%)`,
          padding: "var(--space-6)",
        }}
      >
        {/* Ambient Blurred Backdrop Top */}
        <div
          class="mbl-item-detail-backdrop"
          style={{
            position: "relative",
            width: "100%",
            height: "360px",
            "border-radius": "var(--radius-md)",
            overflow: "hidden",
            "margin-bottom": "-120px",
          }}
        >
          <Show when={backdropUrl()}>
            <img
              src={backdropUrl() || ""}
              alt=""
              style={{
                width: "100%",
                height: "100%",
                "object-fit": "cover",
                opacity: 0.55,
              }}
            />
          </Show>
          <div
            style={{
              position: "absolute",
              inset: 0,
              background: "linear-gradient(to top, var(--bg-base) 5%, rgba(12, 14, 20, 0.4) 100%)",
            }}
          />
        </div>

        {/* Details Container */}
        <div
          class="mbl-item-detail-layout"
          style={{
            position: "relative",
            display: "flex",
            gap: "var(--space-8)",
            padding: "0 var(--space-4)",
            "z-index": 2,
            "flex-wrap": "wrap",
          }}
        >
          {/* Left Poster Card & Primary Actions */}
          <div
            class="mbl-item-detail-poster-col"
            style={{
              width: "240px",
              "flex-shrink": 0,
              display: "flex",
              "flex-direction": "column",
              gap: "var(--space-4)",
            }}
          >
            <div
              style={{
                width: "100%",
                "aspect-ratio": "2 / 3",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-lg)",
                overflow: "hidden",
                "box-shadow": "var(--shadow-xl)",
              }}
            >
              <Show
                when={posterUrl()}
                fallback={
                  <div
                    style={{
                      width: "100%",
                      height: "100%",
                      display: "flex",
                      "align-items": "center",
                      "justify-content": "center",
                      "font-size": "36px",
                    }}
                  >
                    🎬
                  </div>
                }
              >
                <img
                  src={posterUrl() || ""}
                  alt={item()?.title}
                  style={{ width: "100%", height: "100%", "object-fit": "cover" }}
                />
              </Show>
            </div>

            <Show when={(item()?.versions.length ?? 0) > 1}>
              <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
                <label
                  for="media-version"
                  style={{
                    "font-size": "var(--font-size-xs)",
                    "font-weight": "var(--font-weight-semibold)",
                    color: "var(--text-secondary)",
                  }}
                >
                  Version
                </label>
                <select
                  id="media-version"
                  value={selectedVersionId() ?? ""}
                  style={{ width: "100%" }}
                  onChange={(e) => {
                    setSelectedVersionId(
                      e.currentTarget.value ? Number(e.currentTarget.value) : null,
                    );
                    setSelectedAudioIdx(null);
                    setSelectedSubIdx(null);
                  }}
                >
                  <option value="">Automatic</option>
                  <For each={item()?.versions}>
                    {(version) => (
                      <option value={version.id}>
                        {version.width ?? ""}×{version.height ?? ""} {version.videoCodec ?? ""}{" "}
                        {version.container ?? ""}
                      </option>
                    )}
                  </For>
                </select>
              </div>
            </Show>
            {/* Action Buttons below poster */}
            <div style={{ display: "flex", "flex-direction": "column", gap: "var(--space-3)" }}>
              <Button
                size="lg"
                variant="primary"
                disabled={!playable()}
                onClick={() => handlePlay(resumeAt())}
              >
                <span>▶</span>
                <span>{resumeAt() > 0 ? t("resume") : t("play")}</span>
                <Show when={isSeries() && nextCode()}>
                  <span>{nextCode()}</span>
                </Show>
              </Button>

              {resumeAt() > 0 && (
                <Button size="md" variant="secondary" onClick={() => handlePlay(0)}>
                  {t("fromBeginning")}
                </Button>
              )}

              <Button
                size="md"
                variant="secondary"
                disabled={!playable()}
                onClick={() => handlePlay(resumeAt(), true)}
              >
                <span>⎚</span>
                <span>Cast to TV</span>
              </Button>
              <Show when={!isSeries() && primaryPart()}>
                <PlayOnDevice itemId={itemId()} startMs={Math.max(0, viewOffsetMs())} />
              </Show>
              <Show when={isSeries() && nextEpisode()}>
                {(next) => <PlayOnDevice itemId={next().episode.id} startMs={next().startMs} />}
              </Show>

              <Button
                size="md"
                variant="outline"
                onClick={handleToggleWatched}
                disabled={watchUpdating()}
              >
                <span>{isWatched() ? "✓" : "○"}</span>
                <span>{isWatched() ? "Watched" : "Mark as Watched"}</span>
              </Button>
              <Show when={user()?.isAdmin}>
                <MetadataEditor itemId={itemId()} onSaved={() => void loadData()} />
              </Show>
              <Show
                when={
                  subtitlesAvailable() && (item()?.kind === "movie" || item()?.kind === "episode")
                }
              >
                <Button size="md" variant="outline" onClick={() => setFindingSubtitles(true)}>
                  Find subtitles
                </Button>
              </Show>
              <Show when={findingSubtitles()}>
                <Suspense>
                  <SubtitleSearch
                    itemId={itemId()}
                    title={item()?.title ?? ""}
                    onClose={() => setFindingSubtitles(false)}
                    onDownloaded={() => void loadData()}
                  />
                </Suspense>
              </Show>
              <Show when={user()?.isAdmin && (item()?.kind === "movie" || item()?.kind === "show")}>
                <ArrLinkButton
                  size="md"
                  service={item()?.kind === "show" ? "sonarr" : "radarr"}
                  target={{ item: itemId() }}
                />
              </Show>
            </div>

            {/* Playback Prediction Chip (M3.7) */}
            <Show when={preview()}>
              {(p) => {
                const method = () => p().method;
                const badgeVariant = () =>
                  method() === "directPlay"
                    ? "direct-play"
                    : method() === "directStream"
                      ? "direct-stream"
                      : "transcode";
                const badgeLabel = () =>
                  method() === "directPlay"
                    ? "⚡ Direct Play"
                    : method() === "directStream"
                      ? "🔄 Direct Stream"
                      : "⚙ Transcode";

                return (
                  <div
                    style={{
                      padding: "var(--space-3)",
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border)",
                      "border-radius": "var(--radius-md)",
                    }}
                  >
                    <div
                      style={{
                        display: "flex",
                        "align-items": "center",
                        "justify-content": "space-between",
                        "margin-bottom": "4px",
                      }}
                    >
                      <span
                        style={{
                          "font-size": "11px",
                          color: "var(--text-muted)",
                          "text-transform": "uppercase",
                          "font-weight": "bold",
                        }}
                      >
                        Playback Engine
                      </span>
                      <Badge variant={badgeVariant()}>{badgeLabel()}</Badge>
                    </div>
                    <Show when={p().reasons.length > 0}>
                      <div
                        style={{
                          "font-size": "11px",
                          color: "var(--text-secondary)",
                          "margin-top": "4px",
                        }}
                      >
                        {p().reasons[0]}
                      </div>
                    </Show>
                  </div>
                );
              }}
            </Show>
          </div>

          {/* Right Info Details */}
          <div
            class="mbl-item-detail-info-col"
            style={{ flex: 1, "min-width": "280px", "margin-top": "var(--space-6)" }}
          >
            <button
              type="button"
              onClick={() => navigate(`/library/${item()?.libraryId}`)}
              style={{
                background: "transparent",
                border: "none",
                color: "var(--text-secondary)",
                "font-size": "var(--font-size-sm)",
                cursor: "pointer",
                padding: 0,
                "margin-bottom": "var(--space-2)",
              }}
            >
              &larr; Back to Library
            </button>

            <h1
              style={{
                margin: "0 0 8px",
                "font-size": "clamp(26px, 4vw, 38px)",
                "font-weight": "800",
              }}
            >
              {item()?.title}
            </h1>

            <Show when={item()?.tagline}>
              <div
                style={{
                  "font-size": "var(--font-size-md)",
                  "font-style": "italic",
                  color: "var(--text-muted)",
                  "margin-bottom": "var(--space-4)",
                }}
              >
                "{item()?.tagline}"
              </div>
            </Show>

            {/* Badges / Meta row with Media Tags & "Ends at" time */}
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "var(--space-2)",
                "font-size": "var(--font-size-sm)",
                color: "var(--text-secondary)",
                "margin-bottom": "var(--space-4)",
                "flex-wrap": "wrap",
              }}
            >
              {item()?.originallyAvailableAt && (
                <span>{item()?.originallyAvailableAt?.substring(0, 4)}</span>
              )}

              {item()?.durationMs && (
                <>
                  <span>•</span>
                  <span>{formatDuration(item()?.durationMs)}</span>
                  <span>•</span>
                  <span style={{ color: "var(--accent)" }}>
                    {t("endsAt")} {endsAtTime(item()?.durationMs)}
                  </span>
                </>
              )}

              {item()?.contentRating && <Badge variant="default">{item()?.contentRating}</Badge>}
              {item()?.rating && <Badge variant="warning">★ {item()?.rating?.toFixed(1)}</Badge>}

              {/* Resolution & Codec Badges */}
              {(primaryVersion()?.width ?? 0) >= 3800 && (
                <Badge variant="accent">4K Ultra HD</Badge>
              )}
              {primaryVersion()?.hdr && <Badge variant="accent">{primaryVersion()?.hdr}</Badge>}
              {primaryVersion()?.videoCodec && (
                <Badge variant="default">{primaryVersion()?.videoCodec?.toUpperCase()}</Badge>
              )}
              {primaryVersion()?.audioCodec && (
                <Badge variant="default">
                  {primaryVersion()?.audioCodec?.toUpperCase()} {primaryVersion()?.audioChannels}ch
                </Badge>
              )}
            </div>

            {/* Genres */}
            <Show when={(item()?.genres.length ?? 0) > 0}>
              <div
                style={{
                  display: "flex",
                  gap: "var(--space-2)",
                  "margin-bottom": "var(--space-6)",
                  "flex-wrap": "wrap",
                }}
              >
                <For each={item()?.genres}>{(g) => <Chip label={g} />}</For>
              </div>
            </Show>

            {/* Overview */}
            <div style={{ "margin-bottom": "var(--space-6)" }}>
              <h3
                style={{
                  margin: "0 0 8px",
                  "font-size": "var(--font-size-md)",
                  "font-weight": "var(--font-weight-bold)",
                }}
              >
                Overview
              </h3>
              <p
                style={{
                  margin: 0,
                  "font-size": "var(--font-size-sm)",
                  "line-height": "1.6",
                  color: "var(--text-secondary)",
                  "max-width": "800px",
                }}
              >
                {item()?.summary || "No description provided."}
              </p>
            </div>

            {/* Stream Pickers before playback (Audio / Subtitles) */}
            <div
              style={{
                display: "flex",
                gap: "var(--space-6)",
                "margin-bottom": "var(--space-6)",
                "flex-wrap": "wrap",
              }}
            >
              {primaryPart() && (
                <div>
                  <label
                    for="quality-select"
                    style={{
                      display: "block",
                      "font-size": "var(--font-size-xs)",
                      "font-weight": "bold",
                      color: "var(--text-muted)",
                      "margin-bottom": "4px",
                    }}
                  >
                    Quality
                  </label>
                  <select
                    id="quality-select"
                    value={selectedQuality()}
                    onChange={(e) => setSelectedQuality(Number(e.currentTarget.value))}
                    style={{
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border)",
                      color: "var(--text-primary)",
                      padding: "6px 10px",
                      "border-radius": "var(--radius-sm)",
                    }}
                  >
                    <option value={0}>Automatic</option>
                    <option value={2000000}>2 Mbps</option>
                    <option value={4000000}>4 Mbps</option>
                    <option value={8000000}>8 Mbps</option>
                    <option value={20000000}>20 Mbps</option>
                  </select>
                </div>
              )}

              {audioStreams().length > 0 && (
                <div>
                  <label
                    for="audio-stream-select"
                    style={{
                      display: "block",
                      "font-size": "var(--font-size-xs)",
                      "font-weight": "bold",
                      color: "var(--text-muted)",
                      "margin-bottom": "4px",
                    }}
                  >
                    {t("audio")}
                  </label>
                  <select
                    id="audio-stream-select"
                    value={selectedAudioIdx() ?? ""}
                    onChange={(e) =>
                      setSelectedAudioIdx(
                        e.currentTarget.value === "" ? null : Number(e.currentTarget.value),
                      )
                    }
                    style={{
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border)",
                      color: "var(--text-primary)",
                      padding: "6px 10px",
                      "border-radius": "var(--radius-sm)",
                    }}
                  >
                    <option value="">Automatic</option>
                    <For each={audioStreams()}>
                      {(s) => (
                        <option value={s.idx}>
                          {s.language?.toUpperCase() || "Unknown"} ({s.codec} {s.channels}ch)
                        </option>
                      )}
                    </For>
                  </select>
                </div>
              )}

              {subStreams().length > 0 && (
                <div>
                  <label
                    for="subtitle-stream-select"
                    style={{
                      display: "block",
                      "font-size": "var(--font-size-xs)",
                      "font-weight": "bold",
                      color: "var(--text-muted)",
                      "margin-bottom": "4px",
                    }}
                  >
                    {t("subtitles")}
                  </label>
                  <select
                    id="subtitle-stream-select"
                    value={selectedSubIdx() ?? ""}
                    onChange={(e) =>
                      setSelectedSubIdx(
                        e.currentTarget.value ? Number(e.currentTarget.value) : null,
                      )
                    }
                    style={{
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border)",
                      color: "var(--text-primary)",
                      padding: "6px 10px",
                      "border-radius": "var(--radius-sm)",
                    }}
                  >
                    <option value="">Off</option>
                    <For each={subStreams()}>
                      {(s) => (
                        <option value={s.idx}>
                          {s.language?.toUpperCase() || "Subtitle"} ({s.codec}){" "}
                          {s.isForced ? "[Forced]" : ""}
                        </option>
                      )}
                    </For>
                  </select>
                  <details style={{ "margin-top": "6px", "font-size": "var(--font-size-xs)" }}>
                    <summary style={{ cursor: "pointer", color: "var(--text-muted)" }}>
                      Subtitle settings
                    </summary>
                    <SubtitleOptions
                      style={theme.subtitleStyle()}
                      onChange={(patch) =>
                        theme.setSubtitleStyle({ ...theme.subtitleStyle(), ...patch })
                      }
                    />
                  </details>
                </div>
              )}

              <div>
                <label
                  for="companion-select"
                  style={{
                    display: "block",
                    "font-size": "var(--font-size-xs)",
                    "font-weight": "bold",
                    color: "var(--text-muted)",
                    "margin-bottom": "4px",
                  }}
                >
                  AI companion
                </label>
                <select
                  id="companion-select"
                  value={companionChoice()}
                  onChange={(e) => setCompanionChoice(e.currentTarget.value)}
                  style={{
                    background: "var(--bg-surface)",
                    border: "1px solid var(--border)",
                    color: "var(--text-primary)",
                    padding: "6px 10px",
                    "border-radius": "var(--radius-sm)",
                  }}
                >
                  <option value="on:high">Comments often</option>
                  <option value="on:medium">Comments sometimes</option>
                  <option value="on:low">Comments rarely</option>
                  <option value="replies">Only answers me</option>
                  <option value="off">Off</option>
                </select>
              </div>
            </div>

            {/* Cast & Crew */}
            <Show when={(item()?.people.length ?? 0) > 0}>
              <div style={{ "margin-bottom": "var(--space-6)" }}>
                <h3
                  style={{
                    margin: "0 0 10px",
                    "font-size": "var(--font-size-md)",
                    "font-weight": "var(--font-weight-bold)",
                  }}
                >
                  {t("cast")}
                </h3>
                <div style={{ display: "flex", gap: "var(--space-3)", "flex-wrap": "wrap" }}>
                  <For each={item()?.people.slice(0, 10)}>
                    {(person) => (
                      <div
                        style={{
                          padding: "6px 12px",
                          background: "var(--bg-surface)",
                          border: "1px solid var(--border)",
                          "border-radius": "var(--radius-sm)",
                        }}
                      >
                        <div
                          style={{
                            "font-weight": "600",
                            "font-size": "var(--font-size-xs)",
                            color: "var(--text-primary)",
                          }}
                        >
                          {person.name}
                        </div>
                        <div style={{ color: "var(--text-muted)", "font-size": "10px" }}>
                          {person.character || person.role}
                        </div>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </Show>

            <Show when={item()?.kind === "show" || item()?.kind === "season"}>
              <div style={{ "margin-top": "var(--space-6)" }}>
                <RecapPanel itemId={itemId()} />
              </div>
            </Show>
            <Show when={item()?.kind === "show"}>
              <div style={{ "margin-top": "var(--space-4)" }}>
                <MissingEpisodes showId={itemId()} />
              </div>
            </Show>

            <Show when={item()?.kind === "album" || item()?.kind === "artist"}>
              <div
                style={{ display: "flex", gap: "var(--space-3)", "margin-top": "var(--space-6)" }}
              >
                <Button onClick={() => void playAll(false)} disabled={queueBusy()}>
                  ▶ Play all
                </Button>
                <Button
                  variant="secondary"
                  onClick={() => void playAll(true)}
                  disabled={queueBusy()}
                >
                  Shuffle
                </Button>
              </div>
            </Show>

            {/* Children: Seasons / Episodes */}
            <Show when={children().length > 0}>
              <div style={{ "margin-top": "var(--space-8)" }}>
                <h2
                  style={{
                    margin: "0 0 16px",
                    "font-size": "var(--font-size-xl)",
                    "font-weight": "var(--font-weight-bold)",
                  }}
                >
                  {item()?.kind === "show" ? t("seasons") : t("episodes")}
                </h2>
                <div style={{ display: "flex", "flex-direction": "column", gap: "var(--space-3)" }}>
                  <For each={children()}>
                    {(child) => (
                      <div
                        style={{
                          display: "flex",
                          "align-items": "center",
                          "justify-content": "space-between",
                          padding: "14px 18px",
                          background: "var(--bg-surface)",
                          border: "1px solid var(--border)",
                          "border-radius": "var(--radius-md)",
                        }}
                      >
                        <div
                          style={{
                            display: "flex",
                            "align-items": "center",
                            gap: "var(--space-4)",
                          }}
                        >
                          <span
                            style={{
                              "font-size": "var(--font-size-sm)",
                              "font-weight": "bold",
                              color: "var(--text-muted)",
                              width: "32px",
                            }}
                          >
                            {child.idx ? `#${child.idx}` : "-"}
                          </span>
                          <div>
                            <div
                              style={{ "font-weight": "600", "font-size": "var(--font-size-sm)" }}
                            >
                              <a href={`/item/${child.id}`}>{child.title}</a>
                            </div>
                            <div
                              style={{
                                color: "var(--text-muted)",
                                "font-size": "var(--font-size-xs)",
                              }}
                            >
                              {formatDuration(child.durationMs)}
                            </div>
                          </div>
                        </div>

                        <Button
                          size="sm"
                          variant="primary"
                          onClick={() => {
                            if (["show", "season", "artist", "album"].includes(child.kind)) {
                              navigate(`/item/${child.id}`);
                              return;
                            }
                            if (child.kind === "track") {
                              // Play the album from this track onwards.
                              const tracks = children().filter((c) => c.kind === "track");
                              playQueue(
                                tracks.map((t) => ({
                                  itemId: t.id,
                                  initialStartMs: 0,
                                  title: t.title,
                                  subtitle: item()?.title,
                                })),
                                tracks.findIndex((t) => t.id === child.id),
                              );
                              return;
                            }
                            playItem({
                              itemId: child.id,
                              initialStartMs: Math.max(0, child.viewOffsetMs),
                              title: child.title,
                              subtitle: item()?.title,
                            });
                          }}
                        >
                          {["show", "season", "artist", "album"].includes(child.kind)
                            ? child.title
                            : "▶ Play"}
                        </Button>
                        <Show when={child.kind === "track"}>
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() =>
                              addToQueue({
                                itemId: child.id,
                                initialStartMs: 0,
                                title: child.title,
                                subtitle: item()?.title,
                              })
                            }
                          >
                            Add to queue
                          </Button>
                        </Show>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
}

export default ItemDetail;
