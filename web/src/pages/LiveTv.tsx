import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client, { getStoredToken } from "../api/client";
import { guideWindow } from "../api/liveTvTime";
import type { components } from "../api/schema";
import LiveStreamPlayer from "../components/LiveStreamPlayer";
import { Badge } from "../components/ui/Badge";
import { Button } from "../components/ui/Button";
import { Dialog } from "../components/ui/Dialog";
import { SegmentedControl } from "../components/ui/SegmentedControl";
import { useAuth } from "../context/AuthContext";
import { usePlayer } from "../context/PlayerContext";
import { useToast } from "../context/ToastContext";

type EpgProgram = components["schemas"]["EpgProgram"];
type Channel = components["schemas"]["Channel"];
type EpgGridChannel = components["schemas"]["EpgGridChannel"];
type Tuner = components["schemas"]["Tuner"];
type Recording = components["schemas"]["Recording"];
type RecordingRule = components["schemas"]["RecordingRule"];
type VirtualChannel = components["schemas"]["VirtualChannel"];
type VirtualPlayhead = components["schemas"]["VirtualPlayhead"];
type PluginManifest = components["schemas"]["PluginManifest"];
type PluginChannelFeed = components["schemas"]["PluginChannelFeed"];
type PluginItem = components["schemas"]["PluginItem"];

export function LiveTv() {
  const { showToast } = useToast();
  const { playItem } = usePlayer();
  const { user } = useAuth();

  const [activeTab, setActiveTab] = createSignal<"guide" | "tuners" | "recordings" | "virtual">(
    "guide",
  );
  const [liveStream, setLiveStream] = createSignal<components["schemas"]["LiveStreamInfo"] | null>(
    null,
  );
  let liveRequest = 0;
  let liveDisposed = false;
  let liveAbort: AbortController | null = null;
  let renewAbort: AbortController | null = null;
  let renewing = false;
  let liveFailed = false;
  let retryAbort: AbortController | null = null;
  const [retryingId, setRetryingId] = createSignal<number | null>(null);
  const [recoveringId, setRecoveringId] = createSignal<number | null>(null);
  let recoveryAbort: AbortController | null = null;
  const [startingLive, setStartingLive] = createSignal(false);
  const liveMessage = (error: unknown, fallback: string) => {
    const value = error as { error?: { message?: string } } | undefined;
    return value?.error?.message || fallback;
  };
  const releaseChannel = async (channelId: number, sessionId: string) => {
    const abort = new AbortController();
    const timeout = window.setTimeout(() => abort.abort(), 10000);
    try {
      await client.POST("/api/v1/livetv/stream/{channelId}/release", {
        signal: abort.signal,
        params: { path: { channelId }, query: { sessionId } },
      });
    } catch {
    } finally {
      window.clearTimeout(timeout);
    }
  };
  let pendingRelease: Promise<void> = Promise.resolve();
  const queueRelease = (channel: number, id: string) => {
    pendingRelease = Promise.all([pendingRelease, releaseChannel(channel, id)]).then(() => {});
    return pendingRelease;
  };
  const closeLiveStream = () => {
    liveRequest++;
    liveAbort?.abort();
    renewAbort?.abort();
    setStartingLive(false);
    const current = liveStream();
    setLiveStream(null);
    if (current && !liveFailed) queueRelease(current.channelId, current.sessionId);
    return pendingRelease;
  };
  const renewLive = async () => {
    const current = liveStream();
    if (!current || liveFailed || liveDisposed || renewing) return;
    renewing = true;
    const abort = new AbortController();
    renewAbort = abort;
    const timeout = window.setTimeout(() => abort.abort(), 10000);
    try {
      const result = await client.GET("/api/v1/livetv/stream/{channelId}", {
        signal: abort.signal,
        params: { path: { channelId: current.channelId }, query: { sessionId: current.sessionId } },
      });
      if (liveDisposed || liveStream()?.sessionId !== current.sessionId) {
        if (result.data) void queueRelease(result.data.channelId, result.data.sessionId);
        return;
      }
      if (result.error || !result.data)
        throw new Error(
          result.response.status === 403
            ? "This programme is unavailable to your profile. Select an allowed programme."
            : liveMessage(result.error, "Stream reservation expired. Select the channel again."),
        );
    } catch (error) {
      if (!liveDisposed && liveStream()?.sessionId === current.sessionId && !liveFailed) {
        closeLiveStream();
        showToast({
          title: "Live TV",
          message: error instanceof Error ? error.message : "Could not renew live playback.",
          type: "error",
        });
      }
    } finally {
      window.clearTimeout(timeout);
      if (renewAbort === abort) renewAbort = null;
      renewing = false;
    }
  };
  const leaseTimer = window.setInterval(() => {
    void renewLive();
  }, 15000);
  createEffect(() => {
    const end = liveStream()?.accessEndsAt;
    if (!end) return;
    const timer = window.setTimeout(
      () => {
        if (liveFailed) return;
        void closeLiveStream();
        void loadEpg();
        showToast({
          title: "Live TV",
          message: "This programme has ended. Select an allowed programme to continue watching.",
          type: "info",
        });
      },
      Math.max(0, end - Date.now()),
    );
    onCleanup(() => window.clearTimeout(timer));
  });
  const liveFailure = (message: string) => {
    const current = liveStream();
    if (!current || liveFailed) return;
    liveFailed = true;
    renewAbort?.abort();
    void queueRelease(current.channelId, current.sessionId);
    showToast({ title: "Live TV", message, type: "error" });
  };
  onCleanup(() => {
    liveDisposed = true;
    retryAbort?.abort();
    recoveryAbort?.abort();
    clearInterval(leaseTimer);
    closeLiveStream();
  });
  const [loading, setLoading] = createSignal(false);

  // ---------------------------------------------------------------------------
  // EPG State
  // ---------------------------------------------------------------------------
  // Use the same Unix-millisecond units as XMLTV programmes and recordings.
  const getInitialWindow = guideWindow;

  const initial = getInitialWindow();
  const [startTime, setStartTime] = createSignal<number>(initial.windowStart);
  const [endTime, setEndTime] = createSignal<number>(initial.windowEnd);
  const [gridChannels, setGridChannels] = createSignal<EpgGridChannel[]>([]);

  // Guide Search
  const [searchQuery, setSearchQuery] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<EpgProgram[]>([]);
  const [isSearching, setIsSearching] = createSignal(false);

  // Selected Program Modal
  const [selectedProgram, setSelectedProgram] = createSignal<EpgProgram | null>(null);
  const [selectedChannel, setSelectedChannel] = createSignal<Channel | null>(null);
  const [prePadding, setPrePadding] = createSignal(2);
  const [postPadding, setPostPadding] = createSignal(5);

  // ---------------------------------------------------------------------------
  // Tuners State
  // ---------------------------------------------------------------------------
  const [tuners, setTuners] = createSignal<Tuner[]>([]);
  const [channels, setChannels] = createSignal<Channel[]>([]);
  const [m3uModalOpen, setM3uModalOpen] = createSignal(false);
  const [m3uName, setM3uName] = createSignal("");
  const [m3uContent, setM3uContent] = createSignal("");
  const [xmltvModalOpen, setXmltvModalOpen] = createSignal(false);
  const [xmltvContent, setXmltvContent] = createSignal("");

  // ---------------------------------------------------------------------------
  // Recordings State
  // ---------------------------------------------------------------------------
  const [recordings, setRecordings] = createSignal<Recording[]>([]);
  const [recordingFilter, setRecordingFilter] = createSignal<string>("all");
  const [rules, setRules] = createSignal<RecordingRule[]>([]);
  const [ruleModalOpen, setRuleModalOpen] = createSignal(false);
  const [newRuleTitle, setNewRuleTitle] = createSignal("");
  const [newRuleChannelId, setNewRuleChannelId] = createSignal<number | undefined>(undefined);
  const [newRulePrePadding, setNewRulePrePadding] = createSignal(2);
  const [newRulePostPadding, setNewRulePostPadding] = createSignal(5);
  const [newRuleOnlyNew, setNewRuleOnlyNew] = createSignal(true);
  const [newRuleSeriesOnly, setNewRuleSeriesOnly] = createSignal(true);
  const [newRuleSeason, setNewRuleSeason] = createSignal("");
  const [newRuleKeepMax, setNewRuleKeepMax] = createSignal("");
  const [newRuleDeleteWatched, setNewRuleDeleteWatched] = createSignal(false);

  // ---------------------------------------------------------------------------
  // Virtual Channels & Plugins State
  // ---------------------------------------------------------------------------
  const [virtualChannels, setVirtualChannels] = createSignal<VirtualChannel[]>([]);
  const [playheads, setPlayheads] = createSignal<Record<number, VirtualPlayhead>>({});
  const [createVcModalOpen, setCreateVcModalOpen] = createSignal(false);
  const [newVcName, setNewVcName] = createSignal("");
  const [newVcNumber, setNewVcNumber] = createSignal("100");
  const [newVcMode, setNewVcMode] = createSignal<"linear_loop" | "shuffle">("linear_loop");

  const [plugins, setPlugins] = createSignal<PluginManifest[]>([]);
  const [pluginModalOpen, setPluginModalOpen] = createSignal(false);
  const [selectedPlugin, setSelectedPlugin] = createSignal<PluginManifest | null>(null);
  const [pluginFeeds, setPluginFeeds] = createSignal<PluginChannelFeed[]>([]);
  const [activeFeedItems, setActiveFeedItems] = createSignal<PluginItem[]>([]);
  const [activeFeedTitle, setActiveFeedTitle] = createSignal("");

  // ---------------------------------------------------------------------------
  // Fetch Functions
  // ---------------------------------------------------------------------------
  let epgRequest = 0;
  const loadEpg = async () => {
    const request = ++epgRequest;
    setLoading(true);
    try {
      const res = await client.GET("/api/v1/livetv/epg", {
        params: {
          query: {
            startTime: startTime(),
            endTime: endTime(),
            includeHidden: false,
          },
        },
      });
      if (!liveDisposed && request === epgRequest && res.data) {
        setGridChannels(res.data.channels);
      }
    } catch {
      if (!liveDisposed && request === epgRequest)
        showToast({ title: "Error", message: "Failed to load TV guide", type: "error" });
    } finally {
      if (!liveDisposed && request === epgRequest) setLoading(false);
    }
  };
  const guideTimer = window.setInterval(() => {
    if (activeTab() === "guide") void loadEpg();
  }, 30000);
  onCleanup(() => window.clearInterval(guideTimer));

  const loadTuners = async () => {
    try {
      const [tunersRes, channelsRes] = await Promise.all([
        client.GET("/api/v1/livetv/tuners"),
        client.GET("/api/v1/livetv/channels"),
      ]);
      if (tunersRes.data) setTuners(tunersRes.data);
      if (channelsRes.data) setChannels(channelsRes.data);
    } catch {
      showToast({ title: "Error", message: "Failed to load tuners & channels", type: "error" });
    }
  };

  let recordingsRequest = 0;
  const loadRecordings = async () => {
    const request = ++recordingsRequest;
    try {
      const filter = recordingFilter() === "all" ? undefined : recordingFilter();
      const [recRes, ruleRes] = await Promise.all([
        client.GET("/api/v1/livetv/recordings", {
          params: { query: { status: filter } },
        }),
        client.GET("/api/v1/livetv/rules"),
      ]);
      if (liveDisposed || request !== recordingsRequest) return;
      if (recRes.error || ruleRes.error) throw new Error("Request failed");
      if (recRes.data) setRecordings(recRes.data);
      if (ruleRes.data) setRules(ruleRes.data);
    } catch {
      if (!liveDisposed && request === recordingsRequest)
        showToast({ title: "Error", message: "Failed to load recordings", type: "error" });
    }
  };

  const loadVirtualAndPlugins = async () => {
    try {
      const [vcRes, plugRes] = await Promise.all([
        client.GET("/api/v1/livetv/virtual-channels"),
        client.GET("/api/v1/plugins"),
      ]);
      if (vcRes.data) {
        setVirtualChannels(vcRes.data);
        // Load playheads for each virtual channel
        const playheadMap: Record<number, VirtualPlayhead> = {};
        for (const vc of vcRes.data) {
          try {
            const phRes = await client.GET("/api/v1/livetv/virtual-channels/{id}/playhead", {
              params: { path: { id: vc.id } },
            });
            if (phRes.data) {
              playheadMap[vc.id] = phRes.data;
            }
          } catch {
            // ignore individual playhead error
          }
        }
        setPlayheads(playheadMap);
      }
      if (plugRes.data) setPlugins(plugRes.data);
    } catch {
      showToast({
        title: "Error",
        message: "Failed to load virtual channels & plugins",
        type: "error",
      });
    }
  };

  onMount(() => {
    loadEpg();
    loadTuners();
    loadRecordings();
    loadVirtualAndPlugins();
  });
  const recordingsTimer = window.setInterval(() => {
    if (activeTab() === "recordings") void loadRecordings();
  }, 10000);
  onCleanup(() => clearInterval(recordingsTimer));

  // ---------------------------------------------------------------------------
  // Guide Navigation & Actions
  // ---------------------------------------------------------------------------
  const shiftWindow = (hours: number) => {
    const shiftMs = hours * 3_600_000;
    setStartTime((s) => s + shiftMs);
    setEndTime((e) => e + shiftMs);
    loadEpg();
  };

  const resetToNow = () => {
    const init = getInitialWindow();
    setStartTime(init.windowStart);
    setEndTime(init.windowEnd);
    loadEpg();
  };

  const handleSearchGuide = async () => {
    const q = searchQuery().trim();
    if (!q) {
      setSearchResults([]);
      setIsSearching(false);
      return;
    }
    setIsSearching(true);
    try {
      const res = await client.GET("/api/v1/livetv/epg/search", {
        params: { query: { query: q, limit: 30 } },
      });
      if (res.data) setSearchResults(res.data);
    } catch {
      showToast({ title: "Search Error", message: "Failed to search guide", type: "error" });
    }
  };

  const handleScheduleRecording = async (program: EpgProgram, channelId?: number) => {
    const cid = channelId || program.channelId;
    if (!cid) {
      showToast({
        title: "Error",
        message: "Program does not have an assigned channel",
        type: "error",
      });
      return;
    }
    try {
      const result = await client.POST("/api/v1/livetv/recordings", {
        body: {
          channelId: cid,
          programId: program.id,
          title: program.title,
          startTime: program.startTime,
          endTime: program.endTime,
          startPaddingSecs: prePadding() * 60,
          endPaddingSecs: postPadding() * 60,
        },
      });
      if (result.error) throw new Error(liveMessage(result.error, "Request failed"));
      showToast({
        title: "Scheduled",
        message: `Recording scheduled for "${result.data?.title ?? program.title}"`,
        type: "success",
      });
      setSelectedProgram(null);
      loadRecordings();
    } catch (error) {
      showToast({
        title: "Error",
        message: error instanceof Error ? error.message : "Failed to schedule recording",
        type: "error",
      });
    }
  };

  const handleCreateSeriesRule = async (program: EpgProgram, channelId?: number) => {
    try {
      const result = await client.POST("/api/v1/livetv/rules", {
        body: {
          title: program.title,
          channelId: channelId || program.channelId,
          startPaddingSecs: prePadding() * 60,
          endPaddingSecs: postPadding() * 60,
          newOnly: true,
          seriesOnly: true,
        },
      });
      if (result.error) throw new Error("Request failed");
      showToast({
        title: "Series Rule Created",
        message: `Rule created for "${program.title}"`,
        type: "success",
      });
      setSelectedProgram(null);
      loadRecordings();
    } catch {
      showToast({ title: "Error", message: "Failed to create series rule", type: "error" });
    }
  };

  const handleWatchLive = async (channelId: number) => {
    const releasing = closeLiveStream();
    const request = ++liveRequest;
    const abort = new AbortController();
    liveAbort = abort;
    let timeout: number | undefined;
    liveFailed = false;
    setStartingLive(true);
    try {
      await releasing;
      if (liveDisposed || request !== liveRequest || abort.signal.aborted) return;
      timeout = window.setTimeout(() => abort.abort(), 20000);
      const res = await client.GET("/api/v1/livetv/stream/{channelId}", {
        signal: abort.signal,
        params: { path: { channelId } },
      });
      if (liveDisposed || request !== liveRequest) {
        if (res.data) void queueRelease(res.data.channelId, res.data.sessionId);
        return;
      }
      if (res.error || !res.data)
        throw new Error(
          res.response.status === 403
            ? "No current programme is available to your profile. A permitted TV guide programme is required."
            : liveMessage(res.error, "Live source or encoder capacity unavailable."),
        );
      setLiveStream(res.data);
      setSelectedProgram(null);
    } catch (error) {
      if (!liveDisposed && request === liveRequest)
        showToast({
          title: "Live TV unavailable",
          message: error instanceof Error ? error.message : "Could not start live playback.",
          type: "error",
        });
    } finally {
      window.clearTimeout(timeout);
      if (request === liveRequest) {
        liveAbort = null;
        if (!liveDisposed) setStartingLive(false);
      }
    }
  };

  // ---------------------------------------------------------------------------
  // Tuner Actions
  // ---------------------------------------------------------------------------
  const handleDiscoverTuners = async () => {
    try {
      const res = await client.POST("/api/v1/livetv/tuners/discover");
      const count = Array.isArray(res.data) ? res.data.length : 0;
      showToast({
        title: "Discovery Complete",
        message: `Discovered ${count} HDHomeRun tuner(s)`,
        type: "info",
      });
      loadTuners();
    } catch {
      showToast({ title: "Error", message: "Failed to discover HDHomeRun tuners", type: "error" });
    }
  };

  const handleImportM3u = async () => {
    if (!m3uName().trim() || !m3uContent().trim()) {
      showToast({
        title: "Validation",
        message: "Name and M3U content are required",
        type: "warning",
      });
      return;
    }
    try {
      const res = await client.POST("/api/v1/livetv/tuners/m3u", {
        body: { name: m3uName().trim(), content: m3uContent().trim() },
      });
      showToast({
        title: "M3U Imported",
        message: `Successfully configured tuner "${res.data?.name ?? m3uName()}"`,
        type: "success",
      });
      setM3uModalOpen(false);
      setM3uName("");
      setM3uContent("");
      loadTuners();
      loadEpg();
    } catch {
      showToast({ title: "Import Error", message: "Failed to parse M3U playlist", type: "error" });
    }
  };

  const handleImportXmltv = async () => {
    if (!xmltvContent().trim()) {
      showToast({ title: "Validation", message: "XMLTV content is required", type: "warning" });
      return;
    }
    try {
      const token = getStoredToken();
      const headers: Record<string, string> = { "Content-Type": "application/xml" };
      if (token) headers.Authorization = `Bearer ${token}`;

      const res = await fetch("/api/v1/livetv/epg/xmltv", {
        method: "POST",
        headers,
        body: xmltvContent().trim(),
      });
      if (!res.ok) throw new Error(`HTTP error ${res.status}`);
      const data = await res.json();
      showToast({
        title: "XMLTV Imported",
        message: `Imported ${data?.count ?? 0} programmes`,
        type: "success",
      });
      setXmltvModalOpen(false);
      setXmltvContent("");
      loadEpg();
    } catch {
      showToast({ title: "Import Error", message: "Failed to parse XMLTV guide", type: "error" });
    }
  };

  const handleDeleteTuner = async (id: number) => {
    if (!confirm(`Delete tuner #${id}? Associated channels will also be removed.`)) return;
    try {
      const result = await client.DELETE("/api/v1/livetv/tuners/{id}", {
        params: { path: { id } },
      });
      if (result.error) throw new Error("Request failed");
      showToast({ title: "Tuner Deleted", message: `Tuner #${id} removed`, type: "success" });
      loadTuners();
      loadEpg();
    } catch {
      showToast({ title: "Error", message: "Failed to delete tuner", type: "error" });
    }
  };

  // ---------------------------------------------------------------------------
  // Recording Actions
  // ---------------------------------------------------------------------------
  const handlePlayRecording = async (rec: Recording) => {
    if (!["completed", "partial"].includes(rec.status) || !rec.itemId) return;
    const releasing = closeLiveStream();
    const request = liveRequest;
    await releasing;
    if (liveDisposed || request !== liveRequest) return;
    playItem({
      itemId: rec.itemId,
      title: rec.status === "partial" ? `${rec.title} (Incomplete recording)` : rec.title,
    });
  };
  const handleRetryRecording = async (rec: Recording) => {
    if (!rec.channelId || !rec.canDelete || retryingId() !== null || recoveringId() !== null)
      return;
    const abort = new AbortController();
    retryAbort = abort;
    setRetryingId(rec.id);
    let timeout = window.setTimeout(() => abort.abort(), 10000);
    try {
      const request = {
        retryOf: rec.id,
        channelId: rec.channelId,
        programId: rec.programId,
        title: rec.title,
        startTime: rec.startTime,
        endTime: rec.endTime,
      };
      const preview = await client.POST("/api/v1/livetv/recordings/preview", {
        body: request,
        signal: abort.signal,
      });
      window.clearTimeout(timeout);
      if (liveDisposed || abort.signal.aborted) return;
      const plan = preview.data;
      if (preview.error || !plan)
        throw new Error(liveMessage(preview.error, "Could not preview retry"));
      if (plan.conflict) throw new Error(plan.conflict);
      if (!plan.storage.canStart && !plan.existingRecordingId)
        throw new Error(plan.storage.error || "DVR storage is unavailable");
      if (plan.ruleId && (plan.keepMax || plan.autoDeleteWatched)) {
        const policy = [
          plan.keepMax ? `Keep latest ${plan.keepMax} saved recordings` : "",
          plan.autoDeleteWatched ? "Delete after the owning profile watches" : "",
        ]
          .filter(Boolean)
          .join("; ");
        if (
          !confirm(
            `Retry "${plan.title}" for profile #${plan.ownerId}? Series rule #${plan.ruleId} applies: ${policy}. Saved files may be deleted automatically.`,
          )
        )
          return;
      }
      timeout = window.setTimeout(() => abort.abort(), 10000);
      const result = await client.POST("/api/v1/livetv/recordings", {
        body: request,
        signal: abort.signal,
      });
      if (result.error) throw new Error(liveMessage(result.error, "Could not retry recording"));
      if (!liveDisposed) void loadRecordings();
    } catch (error) {
      if (liveDisposed) return;
      showToast({
        title: "Recording",
        message: abort.signal.aborted
          ? "Retry request timed out. Check recordings before trying again."
          : error instanceof Error
            ? error.message
            : "Could not retry recording",
        type: "error",
      });
    } finally {
      window.clearTimeout(timeout);
      if (retryAbort === abort) retryAbort = null;
      if (!liveDisposed) setRetryingId(null);
    }
  };
  const handleRecoverRecording = async (rec: Recording) => {
    if (!rec.canDelete || recoveringId() !== null || retryingId() !== null) return;
    const abort = new AbortController();
    recoveryAbort = abort;
    setRecoveringId(rec.id);
    let timer = window.setTimeout(() => abort.abort(), 10000);
    try {
      const preview = await client.GET("/api/v1/livetv/recordings/{id}/recovery", {
        params: { path: { id: rec.id } },
        signal: abort.signal,
      });
      window.clearTimeout(timer);
      if (liveDisposed || abort.signal.aborted) return;
      if (preview.error || !preview.data)
        throw new Error(liveMessage(preview.error, "Could not inspect saved media"));
      if (!preview.data.canRecover && !preview.data.existingItemId)
        throw new Error(preview.data.reason || "Recovery is unavailable");
      if (
        !confirm(
          `Recover saved media for "${preview.data.title}" as an incomplete recording? The snapshot may need ${(preview.data.sizeBytes / 1048576).toFixed(1)} MiB of extra space; the original is kept until deletion. No new broadcast is fetched. Existing series retention still applies and may delete the recovered file.`,
        )
      )
        return;
      timer = window.setTimeout(() => abort.abort(), 120000);
      const result = await client.POST("/api/v1/livetv/recordings/{id}/recovery", {
        params: { path: { id: rec.id } },
        signal: abort.signal,
      });
      if (result.error) throw new Error(liveMessage(result.error, "Could not recover saved media"));
      if (!liveDisposed) void loadRecordings();
    } catch (error) {
      if (!liveDisposed)
        showToast({
          title: "Recovery",
          message: abort.signal.aborted
            ? "Recovery request timed out. Check recordings before trying again."
            : error instanceof Error
              ? error.message
              : "Could not recover saved media",
          type: "error",
        });
    } finally {
      window.clearTimeout(timer);
      if (recoveryAbort === abort) recoveryAbort = null;
      if (!liveDisposed) setRecoveringId(null);
    }
  };
  const handleCancelRecording = async (rec: Recording) => {
    const id = rec.id;
    const active = rec.status === "scheduled" || rec.status === "recording";
    if (
      !confirm(
        active
          ? `Cancel recording "${rec.title}"?`
          : `Delete recording "${rec.title}" and its saved file?`,
      )
    )
      return;
    try {
      const result = await client.DELETE("/api/v1/livetv/recordings/{id}", {
        params: { path: { id } },
      });
      if (result.error) throw new Error(liveMessage(result.error, "Could not delete recording"));
      showToast({
        title: active ? "Cancelled" : "Deleted",
        message: `Recording #${id} removed`,
        type: "info",
      });
      loadRecordings();
    } catch (error) {
      showToast({
        title: "Error",
        message: error instanceof Error ? error.message : "Failed to cancel recording",
        type: "error",
      });
    }
  };

  const handleDeleteRule = async (id: number) => {
    if (
      !confirm(
        `Delete series rule #${id}? Pending recordings will be cancelled; current and saved recordings are kept.`,
      )
    )
      return;
    try {
      const result = await client.DELETE("/api/v1/livetv/rules/{id}", {
        params: { path: { id } },
      });
      if (result.error) throw new Error("Request failed");
      showToast({ title: "Rule Deleted", message: `Series rule #${id} removed`, type: "info" });
      loadRecordings();
    } catch {
      showToast({ title: "Error", message: "Failed to delete series rule", type: "error" });
    }
  };

  const handleCreateRuleSubmit = async () => {
    if (!newRuleTitle().trim()) {
      showToast({ title: "Validation", message: "Series title is required", type: "warning" });
      return;
    }
    try {
      const optionalNumber = (value: string, min: number, name: string) => {
        if (!value.trim()) return undefined;
        const n = Number(value);
        if (!Number.isInteger(n) || n < min || n > 10000)
          throw new Error(`${name} must be an integer between ${min} and 10000`);
        return n;
      };
      const result = await client.POST("/api/v1/livetv/rules", {
        body: {
          title: newRuleTitle().trim(),
          channelId: newRuleChannelId(),
          startPaddingSecs: newRulePrePadding() * 60,
          endPaddingSecs: newRulePostPadding() * 60,
          newOnly: newRuleOnlyNew(),
          seriesOnly: newRuleSeriesOnly(),
          seasonNumber: optionalNumber(newRuleSeason(), 0, "Season"),
          keepMax: optionalNumber(newRuleKeepMax(), 1, "Keep latest"),
          autoDeleteWatched: newRuleDeleteWatched(),
        },
      });
      if (result.error) throw new Error(liveMessage(result.error, "Could not create series rule"));
      showToast({
        title: "Rule Created",
        message: `Created rule for "${newRuleTitle()}"`,
        type: "success",
      });
      setRuleModalOpen(false);
      setNewRuleTitle("");
      setNewRuleChannelId(undefined);
      setNewRuleSeason("");
      setNewRuleKeepMax("");
      setNewRuleDeleteWatched(false);
      setNewRuleSeriesOnly(true);
      setNewRuleOnlyNew(true);
      loadRecordings();
    } catch (error) {
      showToast({
        title: "Error",
        message: error instanceof Error ? error.message : "Failed to create series rule",
        type: "error",
      });
    }
  };

  // ---------------------------------------------------------------------------
  // Virtual Channels & Plugins Actions
  // ---------------------------------------------------------------------------
  const handleCreateVirtualChannel = async () => {
    if (!newVcName().trim()) {
      showToast({ title: "Validation", message: "Channel name is required", type: "warning" });
      return;
    }
    try {
      const result = await client.POST("/api/v1/livetv/virtual-channels", {
        body: {
          name: newVcName().trim(),
          channelNumber: newVcNumber().trim(),
          mode: newVcMode(),
          itemIds: [],
          insertCommercials: false,
        },
      });
      if (result.error) throw new Error("Request failed");
      showToast({
        title: "Created",
        message: `Virtual channel "${newVcName()}" created`,
        type: "success",
      });
      setCreateVcModalOpen(false);
      setNewVcName("");
      loadVirtualAndPlugins();
    } catch {
      showToast({ title: "Error", message: "Failed to create virtual channel", type: "error" });
    }
  };

  const handleDeleteVirtualChannel = async (id: number) => {
    if (!confirm(`Delete virtual channel #${id}?`)) return;
    try {
      const result = await client.DELETE("/api/v1/livetv/virtual-channels/{id}", {
        params: { path: { id } },
      });
      if (result.error) throw new Error("Request failed");
      showToast({ title: "Deleted", message: `Virtual channel #${id} removed`, type: "info" });
      loadVirtualAndPlugins();
    } catch {
      showToast({ title: "Error", message: "Failed to delete virtual channel", type: "error" });
    }
  };

  const handleTogglePlugin = async (manifest: PluginManifest, currentlyEnabled: boolean) => {
    try {
      const result = await client.PATCH("/api/v1/plugins/{id}", {
        params: { path: { id: manifest.id } },
        body: { enabled: !currentlyEnabled, settings: {} },
      });
      if (result.error) throw new Error("Request failed");
      showToast({
        title: "Plugin Updated",
        message: `${manifest.name} is now ${!currentlyEnabled ? "enabled" : "disabled"}`,
        type: "success",
      });
      loadVirtualAndPlugins();
    } catch {
      showToast({ title: "Error", message: "Failed to update plugin status", type: "error" });
    }
  };

  const handleOpenPluginFeeds = async (manifest: PluginManifest) => {
    setSelectedPlugin(manifest);
    setPluginModalOpen(true);
    setPluginFeeds([]);
    setActiveFeedItems([]);
    try {
      const res = await client.GET("/api/v1/plugins/{id}/channels", {
        params: { path: { id: manifest.id } },
      });
      if (res.data) {
        setPluginFeeds(res.data);
        if (res.data.length > 0) {
          handleSelectFeed(manifest.id, res.data[0].id, res.data[0].title);
        }
      }
    } catch {
      showToast({ title: "Error", message: "Failed to load plugin channels", type: "error" });
    }
  };

  const handleSelectFeed = async (pluginId: string, feedId: string, feedTitle: string) => {
    setActiveFeedTitle(feedTitle);
    try {
      const res = await client.GET("/api/v1/plugins/{id}/channels/{feedId}", {
        params: { path: { id: pluginId, feedId } },
      });
      if (res.data) setActiveFeedItems(res.data);
    } catch {
      showToast({ title: "Error", message: "Failed to load feed items", type: "error" });
    }
  };

  // ---------------------------------------------------------------------------
  // Time Formatting Helpers
  // ---------------------------------------------------------------------------
  const formatTime = (epochMs: number) => {
    return new Date(epochMs).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  const formatDate = (epochMs: number) => {
    return new Date(epochMs).toLocaleDateString([], {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  return (
    <div style={{ padding: "24px", "max-width": "1400px", margin: "0 auto" }}>
      <Show when={startingLive()}>
        <p role="status">
          Preparing live playback… <Button onClick={closeLiveStream}>Cancel</Button>
        </p>
      </Show>
      <Show keyed when={liveStream()}>
        {(stream) => (
          <LiveStreamPlayer
            url={stream.streamUrl}
            title={stream.channelName}
            onClose={() => {
              if (liveStream()?.sessionId === stream.sessionId) void closeLiveStream();
            }}
            onFailure={(message) => {
              if (liveStream()?.sessionId === stream.sessionId) liveFailure(message);
            }}
            onEnded={() => {
              if (liveStream()?.sessionId === stream.sessionId) void closeLiveStream();
            }}
          />
        )}
      </Show>
      {/* Header & Main Nav Tabs */}
      <div
        style={{
          display: "flex",
          "flex-wrap": "wrap",
          "align-items": "center",
          "justify-content": "space-between",
          gap: "16px",
          "margin-bottom": "24px",
          "border-bottom": "1px solid var(--border)",
          "padding-bottom": "16px",
        }}
      >
        <div>
          <h1 style={{ margin: "0 0 4px 0", "font-size": "26px", "font-weight": "800" }}>
            📺 Live TV & Channels
          </h1>
          <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
            Watch broadcast television, manage DVR recordings, and stream online media plugins.
            Age-limited profiles record only the guide programme window, without extra padding.
          </p>
        </div>

        <SegmentedControl
          value={activeTab()}
          onChange={(tab) => setActiveTab(tab as "guide" | "tuners" | "recordings" | "virtual")}
          options={[
            { value: "guide", label: "EPG TV Guide" },
            { value: "tuners", label: "Tuners & Sources" },
            { value: "recordings", label: "DVR Recordings" },
            { value: "virtual", label: "Virtual & Plugins" },
          ]}
        />
      </div>

      {/* =======================================================================
          TAB 1: EPG TV GUIDE
          ======================================================================= */}
      <Show when={activeTab() === "guide"}>
        <div style={{ display: "flex", "flex-direction": "column", gap: "20px" }}>
          {/* Guide Controls Bar */}
          <div
            style={{
              display: "flex",
              "flex-wrap": "wrap",
              "align-items": "center",
              "justify-content": "space-between",
              gap: "12px",
              background: "var(--bg-surface)",
              padding: "12px 16px",
              "border-radius": "var(--radius-md)",
              border: "1px solid var(--border)",
            }}
          >
            {/* Time Shift Buttons */}
            <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
              <Button variant="secondary" onClick={() => shiftWindow(-2)}>
                ◀ -2h
              </Button>
              <Button variant="secondary" onClick={resetToNow}>
                Now
              </Button>
              <Button variant="secondary" onClick={() => shiftWindow(2)}>
                +2h ▶
              </Button>
              <span style={{ "font-size": "14px", "font-weight": "600", "margin-left": "8px" }}>
                {formatTime(startTime())} – {formatTime(endTime())}
              </span>
            </div>

            {/* Guide Search Bar */}
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "8px",
                flex: 1,
                "max-width": "400px",
              }}
            >
              <input
                type="text"
                placeholder="Search TV Guide..."
                value={searchQuery()}
                onInput={(e) => setSearchQuery(e.currentTarget.value)}
                onKeyDown={(e) => e.key === "Enter" && handleSearchGuide()}
                style={{
                  flex: 1,
                  padding: "8px 12px",
                  "border-radius": "var(--radius-sm)",
                  border: "1px solid var(--border)",
                  background: "var(--bg-card)",
                  color: "var(--text-primary)",
                  "font-size": "13px",
                }}
              />
              <Button variant="primary" onClick={handleSearchGuide}>
                Search
              </Button>
              <Show when={isSearching()}>
                <Button
                  variant="ghost"
                  onClick={() => {
                    setSearchQuery("");
                    setIsSearching(false);
                  }}
                >
                  Clear
                </Button>
              </Show>
            </div>
          </div>

          {/* Search Results Display */}
          <Show when={isSearching()}>
            <div
              style={{
                background: "var(--bg-surface)",
                padding: "16px",
                "border-radius": "var(--radius-md)",
                border: "1px solid var(--border)",
              }}
            >
              <h3 style={{ margin: "0 0 12px 0", "font-size": "16px" }}>
                Guide Search Results ({searchResults().length})
              </h3>
              <Show
                when={searchResults().length > 0}
                fallback={
                  <div style={{ color: "var(--text-muted)" }}>No programs matched your query.</div>
                }
              >
                <div style={{ display: "flex", "flex-direction": "column", gap: "8px" }}>
                  <For each={searchResults()}>
                    {(prog) => (
                      <div
                        style={{
                          display: "flex",
                          "align-items": "center",
                          "justify-content": "space-between",
                          padding: "10px 14px",
                          background: "var(--bg-card)",
                          "border-radius": "var(--radius-sm)",
                          border: "1px solid var(--border)",
                        }}
                      >
                        <div>
                          <div style={{ "font-weight": "700" }}>{prog.title}</div>
                          <div style={{ "font-size": "12px", color: "var(--text-muted)" }}>
                            {formatDate(prog.startTime)} – {formatTime(prog.endTime)}
                            {prog.genres &&
                              prog.genres.length > 0 &&
                              ` • ${prog.genres.join(", ")}`}
                          </div>
                          {prog.description && (
                            <div
                              style={{
                                "font-size": "12px",
                                color: "var(--text-secondary)",
                                "margin-top": "4px",
                              }}
                            >
                              {prog.description.slice(0, 120)}...
                            </div>
                          )}
                        </div>
                        <div style={{ display: "flex", gap: "8px" }}>
                          <Button
                            variant="secondary"
                            onClick={() => {
                              setSelectedProgram(prog);
                              setSelectedChannel(null);
                            }}
                          >
                            Details
                          </Button>
                          <Button variant="primary" onClick={() => handleScheduleRecording(prog)}>
                            Record
                          </Button>
                        </div>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          </Show>

          {/* 2-Axis EPG Grid */}
          <div
            style={{
              background: "var(--bg-surface)",
              "border-radius": "var(--radius-md)",
              border: "1px solid var(--border)",
              overflow: "hidden",
            }}
          >
            {/* Timeline Header Row */}
            <div
              style={{
                display: "grid",
                "grid-template-columns": "200px 1fr",
                "border-bottom": "2px solid var(--border)",
                background: "var(--bg-card)",
              }}
            >
              <div
                style={{
                  padding: "12px 16px",
                  "font-weight": "700",
                  "font-size": "13px",
                  color: "var(--text-secondary)",
                  "border-right": "1px solid var(--border)",
                }}
              >
                Channel
              </div>
              <div
                style={{
                  display: "grid",
                  "grid-template-columns": "repeat(4, 1fr)",
                  "text-align": "left",
                  padding: "12px 0",
                  "font-size": "12px",
                  "font-weight": "600",
                  color: "var(--text-secondary)",
                }}
              >
                <div style={{ "padding-left": "8px" }}>{formatTime(startTime())}</div>
                <div style={{ "padding-left": "8px" }}>{formatTime(startTime() + 1_800_000)}</div>
                <div style={{ "padding-left": "8px" }}>{formatTime(startTime() + 3_600_000)}</div>
                <div style={{ "padding-left": "8px" }}>{formatTime(startTime() + 5_400_000)}</div>
              </div>
            </div>

            {/* Channels & Programs Rows */}
            <Show
              when={gridChannels().length > 0}
              fallback={
                <div
                  style={{ padding: "40px", "text-align": "center", color: "var(--text-muted)" }}
                >
                  {loading()
                    ? "Loading TV guide..."
                    : "No channels found. Add an M3U playlist or discover HDHomeRun tuners in the Tuners tab."}
                </div>
              }
            >
              <For each={gridChannels()}>
                {(gridChan) => {
                  const chan = gridChan.channel;
                  const totalWindowMs = endTime() - startTime();

                  return (
                    <div
                      style={{
                        display: "grid",
                        "grid-template-columns": "200px 1fr",
                        "border-bottom": "1px solid var(--border)",
                        "min-height": "70px",
                      }}
                    >
                      {/* Left: Channel Info */}
                      <div
                        style={{
                          padding: "10px 16px",
                          display: "flex",
                          "align-items": "center",
                          "justify-content": "space-between",
                          gap: "8px",
                          "border-right": "1px solid var(--border)",
                          background: "var(--bg-surface)",
                        }}
                      >
                        <div style={{ overflow: "hidden" }}>
                          <div style={{ display: "flex", "align-items": "center", gap: "6px" }}>
                            <Badge variant="accent">{chan.channelNumber}</Badge>
                            <span
                              style={{
                                "font-weight": "700",
                                "font-size": "13px",
                                "white-space": "nowrap",
                                overflow: "hidden",
                                "text-overflow": "ellipsis",
                              }}
                            >
                              {chan.name}
                            </span>
                          </div>
                          {chan.callsign && (
                            <div
                              style={{
                                "font-size": "11px",
                                color: "var(--text-muted)",
                                "margin-top": "2px",
                              }}
                            >
                              {chan.callsign}
                            </div>
                          )}
                        </div>
                        <Button
                          variant="ghost"
                          aria-label={`Watch ${chan.name}`}
                          disabled={chan.isVirtual || chan.canPlay === false}
                          title={
                            chan.isVirtual
                              ? "Virtual channel playback is not available yet."
                              : chan.canPlay === false
                                ? "No current programme is available to your profile."
                                : undefined
                          }
                          onClick={() => handleWatchLive(chan.id)}
                        >
                          ▶
                        </Button>
                      </div>

                      {/* Right: Programs Time Track */}
                      <div
                        style={{
                          position: "relative",
                          display: "flex",
                          height: "100%",
                          overflow: "hidden",
                          background: "var(--bg-base)",
                        }}
                      >
                        <For each={gridChan.programs}>
                          {(prog) => {
                            // Calculate position % inside current window
                            const progStart = Math.max(prog.startTime, startTime());
                            const progEnd = Math.min(prog.endTime, endTime());
                            const durationSecs = progEnd - progStart;
                            if (durationSecs <= 0) return null;

                            const leftPercent = ((progStart - startTime()) / totalWindowMs) * 100;
                            const widthPercent = (durationSecs / totalWindowMs) * 100;

                            return (
                              <button
                                type="button"
                                onClick={() => {
                                  setSelectedProgram(prog);
                                  setSelectedChannel(chan);
                                }}
                                style={{
                                  position: "absolute",
                                  left: `${leftPercent}%`,
                                  width: `${widthPercent}%`,
                                  height: "calc(100% - 6px)",
                                  margin: "3px 0",
                                  padding: "6px 10px",
                                  background: "var(--bg-card)",
                                  border: "1px solid var(--border)",
                                  "border-radius": "var(--radius-sm)",
                                  "text-align": "left",
                                  overflow: "hidden",
                                  cursor: "pointer",
                                  color: "var(--text-primary)",
                                  transition: "background var(--duration-fast)",
                                }}
                              >
                                <div
                                  style={{
                                    "font-size": "12px",
                                    "font-weight": "700",
                                    "white-space": "nowrap",
                                    overflow: "hidden",
                                    "text-overflow": "ellipsis",
                                  }}
                                >
                                  {prog.title}
                                </div>
                                <div
                                  style={{
                                    "font-size": "10px",
                                    color: "var(--text-muted)",
                                    "white-space": "nowrap",
                                    overflow: "hidden",
                                    "text-overflow": "ellipsis",
                                  }}
                                >
                                  {formatTime(prog.startTime)} – {formatTime(prog.endTime)}
                                </div>
                              </button>
                            );
                          }}
                        </For>
                      </div>
                    </div>
                  );
                }}
              </For>
            </Show>
          </div>
        </div>
      </Show>

      {/* =======================================================================
          TAB 2: TUNERS & SOURCES
          ======================================================================= */}
      <Show when={activeTab() === "tuners"}>
        <div style={{ display: "flex", "flex-direction": "column", gap: "24px" }}>
          {/* Action Header */}
          <div
            style={{
              display: "flex",
              "flex-wrap": "wrap",
              "align-items": "center",
              "justify-content": "space-between",
              gap: "12px",
            }}
          >
            <div>
              <h2 style={{ margin: "0 0 4px 0", "font-size": "18px", "font-weight": "700" }}>
                Connected Tuners & Hardware
              </h2>
              <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "13px" }}>
                Discover physical network tuners (HDHomeRun UDP 65001) or import M3U IPTV streams.
              </p>
            </div>
            <Show when={user()?.isAdmin}>
              <div style={{ display: "flex", gap: "8px" }}>
                <Button variant="secondary" onClick={handleDiscoverTuners}>
                  🔍 Discover HDHomeRun
                </Button>
                <Button variant="secondary" onClick={() => setXmltvModalOpen(true)}>
                  📄 Import XMLTV
                </Button>
                <Button variant="primary" onClick={() => setM3uModalOpen(true)}>
                  ➕ Add M3U Tuner
                </Button>
              </div>
            </Show>
          </div>

          {/* Tuner Cards Grid */}
          <Show
            when={tuners().length > 0}
            fallback={
              <div
                style={{
                  padding: "40px",
                  "text-align": "center",
                  background: "var(--bg-surface)",
                  "border-radius": "var(--radius-md)",
                  border: "1px dashed var(--border)",
                  color: "var(--text-muted)",
                }}
              >
                {user()?.isAdmin
                  ? 'No TV tuners configured yet. Click "Discover HDHomeRun" or "Add M3U Tuner" above.'
                  : "No TV tuners configured. Ask your administrator to add a source."}
              </div>
            }
          >
            <div
              style={{
                display: "grid",
                "grid-template-columns": "repeat(auto-fill, minmax(320px, 1fr))",
                gap: "16px",
              }}
            >
              <For each={tuners()}>
                {(tuner) => (
                  <div
                    style={{
                      background: "var(--bg-surface)",
                      padding: "16px",
                      "border-radius": "var(--radius-md)",
                      border: "1px solid var(--border)",
                      display: "flex",
                      "flex-direction": "column",
                      "justify-content": "space-between",
                      gap: "12px",
                    }}
                  >
                    <div>
                      <div
                        style={{
                          display: "flex",
                          "align-items": "center",
                          "justify-content": "space-between",
                          "margin-bottom": "8px",
                        }}
                      >
                        <span style={{ "font-weight": "700", "font-size": "16px" }}>
                          {tuner.name}
                        </span>
                        <Badge variant={tuner.status === "online" ? "success" : "default"}>
                          {tuner.status}
                        </Badge>
                      </div>
                      <div
                        style={{
                          "font-size": "12px",
                          color: "var(--text-secondary)",
                          "word-break": "break-all",
                        }}
                      >
                        <strong>Type:</strong> {tuner.kind.toUpperCase()} • <strong>Tuners:</strong>{" "}
                        {tuner.tunerCount}
                      </div>
                      <div
                        style={{
                          "font-size": "12px",
                          color: "var(--text-muted)",
                          "margin-top": "4px",
                          "word-break": "break-all",
                        }}
                      >
                        {tuner.url}
                      </div>
                    </div>

                    <Show when={user()?.isAdmin}>
                      <div style={{ display: "flex", "justify-content": "flex-end" }}>
                        <Button variant="danger" onClick={() => handleDeleteTuner(tuner.id)}>
                          Delete
                        </Button>
                      </div>
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </Show>

          {/* Channels List */}
          <div>
            <h3 style={{ margin: "0 0 12px 0", "font-size": "16px" }}>
              Tuned Channels ({channels().length})
            </h3>
            <div
              style={{
                background: "var(--bg-surface)",
                "border-radius": "var(--radius-md)",
                border: "1px solid var(--border)",
                overflow: "hidden",
              }}
            >
              <table style={{ width: "100%", "border-collapse": "collapse", "font-size": "13px" }}>
                <thead>
                  <tr
                    style={{
                      background: "var(--bg-card)",
                      "border-bottom": "1px solid var(--border)",
                      "text-align": "left",
                    }}
                  >
                    <th style={{ padding: "10px 16px" }}>#</th>
                    <th style={{ padding: "10px 16px" }}>Name</th>
                    <th style={{ padding: "10px 16px" }}>EPG Channel ID</th>
                    <th style={{ padding: "10px 16px" }}>Status</th>
                    <th style={{ padding: "10px 16px", "text-align": "right" }}>Actions</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={channels()}>
                    {(ch) => (
                      <tr style={{ "border-bottom": "1px solid var(--border)" }}>
                        <td style={{ padding: "10px 16px", "font-weight": "700" }}>
                          {ch.channelNumber}
                        </td>
                        <td style={{ padding: "10px 16px" }}>{ch.name}</td>
                        <td style={{ padding: "10px 16px", color: "var(--text-muted)" }}>
                          {ch.epgChannelId || "—"}
                        </td>
                        <td style={{ padding: "10px 16px" }}>
                          <Show when={ch.isVirtual}>
                            <Badge variant="accent">Virtual</Badge>
                          </Show>
                          <Show when={!ch.isVirtual}>
                            <Badge variant="default">Live</Badge>
                          </Show>
                        </td>
                        <td style={{ padding: "10px 16px", "text-align": "right" }}>
                          <Button
                            variant="ghost"
                            disabled={ch.isVirtual || ch.canPlay === false}
                            title={
                              ch.isVirtual
                                ? "Virtual channel playback is not available yet."
                                : ch.canPlay === false
                                  ? "No current programme is available to your profile."
                                  : undefined
                            }
                            onClick={() => handleWatchLive(ch.id)}
                          >
                            Stream
                          </Button>
                        </td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </Show>

      {/* =======================================================================
          TAB 3: DVR RECORDINGS & RULES
          ======================================================================= */}
      <Show when={activeTab() === "recordings"}>
        <div style={{ display: "flex", "flex-direction": "column", gap: "24px" }}>
          {/* Header & Sub-controls */}
          <div
            style={{
              display: "flex",
              "flex-wrap": "wrap",
              "align-items": "center",
              "justify-content": "space-between",
              gap: "12px",
            }}
          >
            <div>
              <h2 style={{ margin: "0 0 4px 0", "font-size": "18px", "font-weight": "700" }}>
                DVR Recordings & Series Rules
              </h2>
              <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "13px" }}>
                Scheduled background recordings with conflict resolution and automated series
                capture.
              </p>
            </div>
            <div style={{ display: "flex", gap: "8px" }}>
              <Button variant="primary" onClick={() => setRuleModalOpen(true)}>
                ➕ New Series Rule
              </Button>
            </div>
          </div>

          {/* Recordings Section */}
          <div>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                "justify-content": "space-between",
                "margin-bottom": "12px",
              }}
            >
              <h3 style={{ margin: 0, "font-size": "16px" }}>
                Scheduled & Recorded Programs ({recordings().length})
              </h3>
              <div style={{ display: "flex", "flex-wrap": "wrap", gap: "6px" }}>
                <For each={["all", "scheduled", "recording", "completed", "partial", "failed"]}>
                  {(filter) => (
                    <Button
                      variant={recordingFilter() === filter ? "primary" : "secondary"}
                      onClick={() => {
                        setRecordingFilter(filter);
                        loadRecordings();
                      }}
                    >
                      {filter.charAt(0).toUpperCase() + filter.slice(1)}
                    </Button>
                  )}
                </For>
              </div>
            </div>

            <div
              style={{
                background: "var(--bg-surface)",
                "border-radius": "var(--radius-md)",
                border: "1px solid var(--border)",
                overflow: "hidden",
              }}
            >
              <Show
                when={recordings().length > 0}
                fallback={
                  <div
                    style={{ padding: "40px", "text-align": "center", color: "var(--text-muted)" }}
                  >
                    No recordings found matching "{recordingFilter()}".
                  </div>
                }
              >
                <table
                  style={{ width: "100%", "border-collapse": "collapse", "font-size": "13px" }}
                >
                  <thead>
                    <tr
                      style={{
                        background: "var(--bg-card)",
                        "border-bottom": "1px solid var(--border)",
                        "text-align": "left",
                      }}
                    >
                      <th style={{ padding: "10px 16px" }}>Title</th>
                      <th style={{ padding: "10px 16px" }}>Air Date & Time</th>
                      <th style={{ padding: "10px 16px" }}>Status</th>
                      <th style={{ padding: "10px 16px" }}>Rule ID</th>
                      <th style={{ padding: "10px 16px", "text-align": "right" }}>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={recordings()}>
                      {(rec) => {
                        const statusBadgeVariant =
                          rec.status === "completed"
                            ? "success"
                            : rec.status === "recording"
                              ? "accent"
                              : rec.status === "failed" || rec.status === "partial"
                                ? "danger"
                                : "default";

                        return (
                          <tr style={{ "border-bottom": "1px solid var(--border)" }}>
                            <td style={{ padding: "10px 16px", "font-weight": "700" }}>
                              {rec.title}
                              <Show when={rec.channelName}>
                                <div
                                  style={{ "font-size": "12px", color: "var(--text-secondary)" }}
                                >
                                  {rec.channelName}
                                </div>
                              </Show>
                              <Show when={rec.durationMs && rec.durationMs > 0}>
                                <div
                                  style={{ "font-size": "12px", color: "var(--text-secondary)" }}
                                >
                                  {Math.floor((rec.durationMs ?? 0) / 60000)}m{" "}
                                  {Math.floor(((rec.durationMs ?? 0) % 60000) / 1000)}s
                                </div>
                              </Show>
                              {rec.errorMessage && (
                                <div
                                  style={{
                                    "font-size": "11px",
                                    color: "var(--danger)",
                                    "font-weight": "normal",
                                  }}
                                >
                                  {rec.errorMessage}
                                </div>
                              )}
                            </td>
                            <td style={{ padding: "10px 16px", color: "var(--text-secondary)" }}>
                              {formatDate(rec.startTime)} – {formatTime(rec.endTime)}
                            </td>
                            <td style={{ padding: "10px 16px" }}>
                              <Badge variant={statusBadgeVariant}>{rec.status.toUpperCase()}</Badge>
                            </td>
                            <td style={{ padding: "10px 16px", color: "var(--text-muted)" }}>
                              {rec.ruleId ? `#${rec.ruleId}` : "Manual"}
                            </td>
                            <td style={{ padding: "10px 16px", "text-align": "right" }}>
                              <Show
                                when={["completed", "partial"].includes(rec.status) && rec.itemId}
                              >
                                <Button
                                  variant="primary"
                                  aria-label={`Play ${rec.status === "partial" ? "partial " : ""}recording ${rec.title}`}
                                  onClick={() => handlePlayRecording(rec)}
                                >
                                  {rec.status === "partial" ? "Play partial" : "Play"}
                                </Button>
                              </Show>
                              <Show
                                when={["completed", "partial"].includes(rec.status) && !rec.itemId}
                              >
                                <span style={{ color: "var(--text-muted)", "font-size": "12px" }}>
                                  Playback unavailable
                                </span>
                              </Show>
                              <Show when={rec.canDelete}>
                                <Show when={rec.status === "failed"}>
                                  <Button
                                    variant="secondary"
                                    disabled={recoveringId() !== null || retryingId() !== null}
                                    onClick={() => handleRecoverRecording(rec)}
                                  >
                                    {recoveringId() === rec.id
                                      ? "Recovering…"
                                      : "Recover saved media"}
                                  </Button>
                                </Show>
                                <Show
                                  when={
                                    (rec.status === "failed" || rec.status === "partial") &&
                                    rec.channelId &&
                                    rec.endTime > Date.now()
                                  }
                                >
                                  <Button
                                    variant="secondary"
                                    title="Retry preserving the owner, padding and series policy"
                                    disabled={retryingId() !== null || recoveringId() !== null}
                                    onClick={() => handleRetryRecording(rec)}
                                  >
                                    {retryingId() === rec.id ? "Preparing retry…" : "Retry"}
                                  </Button>
                                </Show>
                                <Button
                                  variant="danger"
                                  aria-label={`${rec.status === "scheduled" || rec.status === "recording" ? "Cancel" : "Delete"} recording ${rec.title}`}
                                  onClick={() => handleCancelRecording(rec)}
                                >
                                  {rec.status === "scheduled" || rec.status === "recording"
                                    ? "Cancel"
                                    : "Delete"}
                                </Button>
                              </Show>
                            </td>
                          </tr>
                        );
                      }}
                    </For>
                  </tbody>
                </table>
              </Show>
            </div>
          </div>

          {/* Series Rules Section */}
          <div>
            <h3 style={{ margin: "0 0 12px 0", "font-size": "16px" }}>
              Series Recording Rules ({rules().length})
            </h3>
            <div
              style={{
                background: "var(--bg-surface)",
                "border-radius": "var(--radius-md)",
                border: "1px solid var(--border)",
                overflow: "hidden",
              }}
            >
              <Show
                when={rules().length > 0}
                fallback={
                  <div
                    style={{ padding: "30px", "text-align": "center", color: "var(--text-muted)" }}
                  >
                    No series recording rules created yet.
                  </div>
                }
              >
                <table
                  style={{ width: "100%", "border-collapse": "collapse", "font-size": "13px" }}
                >
                  <thead>
                    <tr
                      style={{
                        background: "var(--bg-card)",
                        "border-bottom": "1px solid var(--border)",
                        "text-align": "left",
                      }}
                    >
                      <th style={{ padding: "10px 16px" }}>Series Title</th>
                      <th style={{ padding: "10px 16px" }}>Channel</th>
                      <th style={{ padding: "10px 16px" }}>Padding</th>
                      <th style={{ padding: "10px 16px" }}>Filter</th>
                      <th style={{ padding: "10px 16px", "text-align": "right" }}>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={rules()}>
                      {(rule) => (
                        <tr style={{ "border-bottom": "1px solid var(--border)" }}>
                          <td style={{ padding: "10px 16px", "font-weight": "700" }}>
                            {rule.title}
                          </td>
                          <td style={{ padding: "10px 16px", color: "var(--text-secondary)" }}>
                            {rule.channelId ? `Channel #${rule.channelId}` : "All Channels"}
                          </td>
                          <td style={{ padding: "10px 16px", color: "var(--text-muted)" }}>
                            -{Math.floor(rule.startPaddingSecs / 60)}m / +
                            {Math.floor(rule.endPaddingSecs / 60)}m
                          </td>
                          <td style={{ padding: "10px 16px" }}>
                            <Show
                              when={rule.newOnly}
                              fallback={<Badge variant="default">All Airings</Badge>}
                            >
                              <Badge variant="accent">New Episodes Only</Badge>
                            </Show>
                            <Show when={rule.seasonNumber != null}>
                              <div>Season {rule.seasonNumber}</div>
                            </Show>
                            <div>
                              {rule.seriesOnly ? "Episodic programmes" : "All matching programmes"}
                            </div>
                            <div>
                              {rule.keepMax ? `Keep latest ${rule.keepMax}` : "Keep all recordings"}
                            </div>
                            <Show when={rule.autoDeleteWatched}>
                              <div>Delete after owner watches</div>
                            </Show>
                          </td>
                          <td style={{ padding: "10px 16px", "text-align": "right" }}>
                            <Button variant="danger" onClick={() => handleDeleteRule(rule.id)}>
                              Delete Rule
                            </Button>
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </Show>
            </div>
          </div>
        </div>
      </Show>

      {/* =======================================================================
          TAB 4: VIRTUAL 24/7 CHANNELS & STREAMING PLUGINS
          ======================================================================= */}
      <Show when={activeTab() === "virtual"}>
        <div style={{ display: "flex", "flex-direction": "column", gap: "32px" }}>
          {/* Virtual Channels Section */}
          <div style={{ display: "flex", "flex-direction": "column", gap: "16px" }}>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                "justify-content": "space-between",
              }}
            >
              <div>
                <h2 style={{ margin: "0 0 4px 0", "font-size": "18px", "font-weight": "700" }}>
                  Virtual 24/7 Linear Channels
                </h2>
                <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "13px" }}>
                  Continuous linear TV channels calculated mathematically with zero idle CPU
                  overhead.
                </p>
              </div>
              <Button variant="primary" onClick={() => setCreateVcModalOpen(true)}>
                ➕ Create Virtual Channel
              </Button>
            </div>

            <Show
              when={virtualChannels().length > 0}
              fallback={
                <div
                  style={{
                    padding: "40px",
                    "text-align": "center",
                    background: "var(--bg-surface)",
                    "border-radius": "var(--radius-md)",
                    border: "1px dashed var(--border)",
                    color: "var(--text-muted)",
                  }}
                >
                  No virtual channels configured. Create one above to assemble a continuous 24/7
                  channel from your library.
                </div>
              }
            >
              <div
                style={{
                  display: "grid",
                  "grid-template-columns": "repeat(auto-fill, minmax(340px, 1fr))",
                  gap: "16px",
                }}
              >
                <For each={virtualChannels()}>
                  {(vc) => {
                    const ph = () => playheads()[vc.id];
                    const progressPercent = () => {
                      const cur = ph();
                      if (!cur || cur.durationSecs <= 0) return 0;
                      return Math.min(100, Math.round((cur.offsetSecs / cur.durationSecs) * 100));
                    };

                    return (
                      <div
                        style={{
                          background: "var(--bg-surface)",
                          padding: "16px",
                          "border-radius": "var(--radius-md)",
                          border: "1px solid var(--border)",
                          display: "flex",
                          "flex-direction": "column",
                          "justify-content": "space-between",
                          gap: "12px",
                        }}
                      >
                        <div>
                          <div
                            style={{
                              display: "flex",
                              "align-items": "center",
                              "justify-content": "space-between",
                              "margin-bottom": "8px",
                            }}
                          >
                            <span style={{ "font-weight": "800", "font-size": "16px" }}>
                              {vc.name}
                            </span>
                            <Badge variant="accent">CH {vc.channelId}</Badge>
                          </div>

                          <div
                            style={{
                              "font-size": "12px",
                              color: "var(--text-secondary)",
                              "margin-bottom": "12px",
                            }}
                          >
                            Mode: <strong>{vc.mode}</strong> • Items:{" "}
                            <strong>{vc.items.length}</strong>
                          </div>

                          {/* Live Playhead Progress Box */}
                          <div
                            style={{
                              background: "var(--bg-card)",
                              padding: "10px 12px",
                              "border-radius": "var(--radius-sm)",
                              border: "1px solid var(--border)",
                            }}
                          >
                            <div
                              style={{
                                "font-size": "11px",
                                color: "var(--text-muted)",
                                "text-transform": "uppercase",
                                "letter-spacing": "0.5px",
                              }}
                            >
                              Now Airing
                            </div>
                            <div
                              style={{
                                "font-weight": "700",
                                "font-size": "13px",
                                margin: "2px 0 6px 0",
                              }}
                            >
                              {ph()?.currentTitle || "Airing schedule"}
                            </div>

                            {/* Progress bar */}
                            <div
                              style={{
                                width: "100%",
                                height: "6px",
                                background: "var(--border)",
                                "border-radius": "3px",
                                overflow: "hidden",
                                "margin-bottom": "4px",
                              }}
                            >
                              <div
                                style={{
                                  width: `${progressPercent()}%`,
                                  height: "100%",
                                  background: "var(--accent)",
                                  transition: "width 1s linear",
                                }}
                              />
                            </div>
                            <div
                              style={{
                                display: "flex",
                                "justify-content": "space-between",
                                "font-size": "10px",
                                color: "var(--text-muted)",
                              }}
                            >
                              <span>{Math.floor((ph()?.offsetSecs || 0) / 60)}m</span>
                              <span>{Math.floor((ph()?.durationSecs || 0) / 60)}m</span>
                            </div>
                          </div>
                        </div>

                        <div
                          style={{
                            display: "flex",
                            "justify-content": "space-between",
                            "align-items": "center",
                            "margin-top": "8px",
                          }}
                        >
                          <Button
                            variant="danger"
                            onClick={() => handleDeleteVirtualChannel(vc.id)}
                          >
                            Delete
                          </Button>
                          <Button
                            variant="primary"
                            disabled
                            title="Virtual channel playback is not available yet."
                          >
                            ▶ Watch Channel
                          </Button>
                        </div>
                      </div>
                    );
                  }}
                </For>
              </div>
            </Show>
          </div>

          {/* Streaming Media Plugins Section */}
          <div style={{ display: "flex", "flex-direction": "column", gap: "16px" }}>
            <div>
              <h2 style={{ margin: "0 0 4px 0", "font-size": "18px", "font-weight": "700" }}>
                Online Media Streaming Plugins
              </h2>
              <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "13px" }}>
                Integrate online services (YouTube, Spotify, WebDrama) directly into the unified
                media server.
              </p>
            </div>

            <div
              style={{
                display: "grid",
                "grid-template-columns": "repeat(auto-fill, minmax(340px, 1fr))",
                gap: "16px",
              }}
            >
              <For each={plugins()}>
                {(plugin) => (
                  <div
                    style={{
                      background: "var(--bg-surface)",
                      padding: "16px",
                      "border-radius": "var(--radius-md)",
                      border: "1px solid var(--border)",
                      display: "flex",
                      "flex-direction": "column",
                      "justify-content": "space-between",
                      gap: "12px",
                    }}
                  >
                    <div>
                      <div
                        style={{
                          display: "flex",
                          "align-items": "center",
                          "justify-content": "space-between",
                          "margin-bottom": "8px",
                        }}
                      >
                        <span style={{ "font-weight": "700", "font-size": "16px" }}>
                          {plugin.name}
                        </span>
                        <div style={{ display: "flex", "align-items": "center", gap: "6px" }}>
                          <Badge variant={plugin.enabled !== false ? "success" : "default"}>
                            {plugin.enabled !== false ? "Active" : "Disabled"}
                          </Badge>
                          <Badge variant="default">v{plugin.version}</Badge>
                        </div>
                      </div>
                      <div
                        style={{
                          "font-size": "13px",
                          color: "var(--text-secondary)",
                          "margin-bottom": "8px",
                        }}
                      >
                        {plugin.description}
                      </div>
                      <div style={{ "font-size": "11px", color: "var(--text-muted)" }}>
                        Author: {plugin.author} • Capabilities: {plugin.capabilities.join(", ")}
                      </div>
                    </div>

                    <div
                      style={{
                        display: "flex",
                        "justify-content": "space-between",
                        "align-items": "center",
                      }}
                    >
                      <Button
                        variant={plugin.enabled !== false ? "secondary" : "primary"}
                        onClick={() => handleTogglePlugin(plugin, plugin.enabled ?? true)}
                      >
                        {plugin.enabled !== false ? "Disable" : "Enable"}
                      </Button>
                      <Button
                        variant="primary"
                        disabled={plugin.enabled === false}
                        onClick={() => handleOpenPluginFeeds(plugin)}
                      >
                        Browse Feeds
                      </Button>
                    </div>
                  </div>
                )}
              </For>
            </div>
          </div>
        </div>
      </Show>

      {/* =======================================================================
          MODAL: PROGRAM DETAILS & SCHEDULING
          ======================================================================= */}
      <Dialog
        open={selectedProgram() !== null}
        onOpenChange={(open) => !open && setSelectedProgram(null)}
        title={selectedProgram()?.title || "Program Details"}
        description={
          selectedChannel()
            ? `${selectedChannel()?.name} (Ch ${selectedChannel()?.channelNumber})`
            : "TV Guide Program"
        }
      >
        <Show when={selectedProgram()}>
          {(prog) => {
            const genres = () => prog().genres;
            return (
              <div style={{ display: "flex", "flex-direction": "column", gap: "16px" }}>
                <div style={{ "font-size": "13px", color: "var(--text-secondary)" }}>
                  <div>
                    <strong>Time:</strong> {formatDate(prog().startTime)} –{" "}
                    {formatTime(prog().endTime)}
                  </div>
                  {prog().rating && (
                    <div>
                      <strong>Rating:</strong> {prog().rating}
                    </div>
                  )}
                  {genres() && (genres()?.length ?? 0) > 0 && (
                    <div>
                      <strong>Genres:</strong> {genres()!.join(", ")}
                    </div>
                  )}
                </div>

                {prog().description && (
                  <div
                    style={{
                      "font-size": "13px",
                      background: "var(--bg-base)",
                      padding: "10px",
                      "border-radius": "var(--radius-sm)",
                    }}
                  >
                    {prog().description}
                  </div>
                )}

                {/* DVR Padding Configuration */}
                <div
                  style={{
                    display: "grid",
                    "grid-template-columns": "1fr 1fr",
                    gap: "12px",
                    background: "var(--bg-base)",
                    padding: "10px",
                    "border-radius": "var(--radius-sm)",
                  }}
                >
                  <div>
                    <label
                      for="prePaddingInput"
                      style={{
                        "font-size": "11px",
                        color: "var(--text-muted)",
                        display: "block",
                        "margin-bottom": "4px",
                      }}
                    >
                      Start Padding (mins)
                    </label>
                    <input
                      id="prePaddingInput"
                      type="number"
                      value={prePadding()}
                      onInput={(e) =>
                        setPrePadding(Number.parseInt(e.currentTarget.value, 10) || 0)
                      }
                      style={{
                        width: "100%",
                        padding: "6px",
                        "border-radius": "var(--radius-sm)",
                        border: "1px solid var(--border)",
                        background: "var(--bg-card)",
                        color: "var(--text-primary)",
                      }}
                    />
                  </div>
                  <div>
                    <label
                      for="postPaddingInput"
                      style={{
                        "font-size": "11px",
                        color: "var(--text-muted)",
                        display: "block",
                        "margin-bottom": "4px",
                      }}
                    >
                      End Padding (mins)
                    </label>
                    <input
                      id="postPaddingInput"
                      type="number"
                      value={postPadding()}
                      onInput={(e) =>
                        setPostPadding(Number.parseInt(e.currentTarget.value, 10) || 0)
                      }
                      style={{
                        width: "100%",
                        padding: "6px",
                        "border-radius": "var(--radius-sm)",
                        border: "1px solid var(--border)",
                        background: "var(--bg-card)",
                        color: "var(--text-primary)",
                      }}
                    />
                  </div>
                </div>

                {/* Actions */}
                <div
                  style={{
                    display: "flex",
                    "justify-content": "flex-end",
                    gap: "8px",
                    "margin-top": "8px",
                  }}
                >
                  <Show when={selectedChannel()}>
                    {(chan) => (
                      <Button
                        variant="secondary"
                        disabled={chan().isVirtual || chan().canPlay === false}
                        title={
                          chan().isVirtual
                            ? "Virtual channel playback is not available yet."
                            : chan().canPlay === false
                              ? "No current programme is available to your profile."
                              : undefined
                        }
                        onClick={() => handleWatchLive(chan().id)}
                      >
                        ▶ Watch Live
                      </Button>
                    )}
                  </Show>
                  <Button
                    variant="secondary"
                    onClick={() => handleCreateSeriesRule(prog(), selectedChannel()?.id)}
                  >
                    Record Series
                  </Button>
                  <Button
                    variant="primary"
                    onClick={() => handleScheduleRecording(prog(), selectedChannel()?.id)}
                  >
                    Record Program
                  </Button>
                </div>
              </div>
            );
          }}
        </Show>
      </Dialog>

      {/* =======================================================================
          MODAL: ADD M3U TUNER
          ======================================================================= */}
      <Dialog
        open={m3uModalOpen()}
        onOpenChange={setM3uModalOpen}
        title="Add M3U Playlist Tuner"
        description="Import channels from an IPTV playlist URL or raw #EXTM3U text."
      >
        <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
          <div>
            <label
              for="m3uNameInput"
              style={{
                "font-size": "12px",
                "font-weight": "600",
                display: "block",
                "margin-bottom": "4px",
              }}
            >
              Tuner Name
            </label>
            <input
              id="m3uNameInput"
              type="text"
              placeholder="e.g. Nordic IPTV"
              value={m3uName()}
              onInput={(e) => setM3uName(e.currentTarget.value)}
              style={{
                width: "100%",
                padding: "8px 10px",
                "border-radius": "var(--radius-sm)",
                border: "1px solid var(--border)",
                background: "var(--bg-card)",
                color: "var(--text-primary)",
              }}
            />
          </div>

          <div>
            <label
              for="m3uContentInput"
              style={{
                "font-size": "12px",
                "font-weight": "600",
                display: "block",
                "margin-bottom": "4px",
              }}
            >
              M3U Content / URL
            </label>
            <textarea
              id="m3uContentInput"
              rows={8}
              placeholder="#EXTM3U&#10;#EXTINF:-1 tvg-id=&quot;svt1&quot;,SVT 1 HD&#10;http://example.com/stream.m3u8"
              value={m3uContent()}
              onInput={(e) => setM3uContent(e.currentTarget.value)}
              style={{
                width: "100%",
                padding: "8px 10px",
                "border-radius": "var(--radius-sm)",
                border: "1px solid var(--border)",
                background: "var(--bg-card)",
                color: "var(--text-primary)",
                "font-family": "monospace",
                "font-size": "12px",
              }}
            />
          </div>

          <div
            style={{
              display: "flex",
              "justify-content": "flex-end",
              gap: "8px",
              "margin-top": "8px",
            }}
          >
            <Button variant="secondary" onClick={() => setM3uModalOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" onClick={handleImportM3u}>
              Import Channels
            </Button>
          </div>
        </div>
      </Dialog>

      {/* =======================================================================
          MODAL: IMPORT XMLTV GUIDE
          ======================================================================= */}
      <Dialog
        open={xmltvModalOpen()}
        onOpenChange={setXmltvModalOpen}
        title="Import XMLTV Guide"
        description="Streamingly parse and populate channel electronic program guides (EPG)."
      >
        <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
          <div>
            <label
              for="xmltvContentInput"
              style={{
                "font-size": "12px",
                "font-weight": "600",
                display: "block",
                "margin-bottom": "4px",
              }}
            >
              XMLTV XML Content
            </label>
            <textarea
              id="xmltvContentInput"
              rows={8}
              placeholder='<?xml version="1.0" encoding="UTF-8"?>&#10;<tv>&#10;  <channel id="svt1"><display-name>SVT1</display-name></channel>&#10;  <programme channel="svt1" start="20261026190000 +0000" stop="20261026200000 +0000">&#10;    <title>Nyheter</title>&#10;  </programme>&#10;</tv>'
              value={xmltvContent()}
              onInput={(e) => setXmltvContent(e.currentTarget.value)}
              style={{
                width: "100%",
                padding: "8px 10px",
                "border-radius": "var(--radius-sm)",
                border: "1px solid var(--border)",
                background: "var(--bg-card)",
                color: "var(--text-primary)",
                "font-family": "monospace",
                "font-size": "12px",
              }}
            />
          </div>

          <div
            style={{
              display: "flex",
              "justify-content": "flex-end",
              gap: "8px",
              "margin-top": "8px",
            }}
          >
            <Button variant="secondary" onClick={() => setXmltvModalOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" onClick={handleImportXmltv}>
              Import Guide
            </Button>
          </div>
        </div>
      </Dialog>

      {/* =======================================================================
          MODAL: NEW SERIES RULE
          ======================================================================= */}
      <Dialog
        open={ruleModalOpen()}
        onOpenChange={setRuleModalOpen}
        title="Create Series Recording Rule"
        description="Automatically capture future airings with conflict handling and padding."
      >
        <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
          <div>
            <label
              for="newRuleTitleInput"
              style={{
                "font-size": "12px",
                "font-weight": "600",
                display: "block",
                "margin-bottom": "4px",
              }}
            >
              Series Title
            </label>
            <input
              id="newRuleTitleInput"
              type="text"
              placeholder="e.g. Aktuellt, Bron, Game of Thrones"
              value={newRuleTitle()}
              onInput={(e) => setNewRuleTitle(e.currentTarget.value)}
              style={{
                width: "100%",
                padding: "8px 10px",
                "border-radius": "var(--radius-sm)",
                border: "1px solid var(--border)",
                background: "var(--bg-card)",
                color: "var(--text-primary)",
              }}
            />
          </div>

          <div>
            <label
              for="newRuleChannelSelect"
              style={{
                "font-size": "12px",
                "font-weight": "600",
                display: "block",
                "margin-bottom": "4px",
              }}
            >
              Channel (Optional)
            </label>
            <select
              id="newRuleChannelSelect"
              value={newRuleChannelId() ?? ""}
              onChange={(e) => {
                const val = e.currentTarget.value;
                setNewRuleChannelId(val ? Number.parseInt(val, 10) : undefined);
              }}
              style={{
                width: "100%",
                padding: "8px 10px",
                "border-radius": "var(--radius-sm)",
                border: "1px solid var(--border)",
                background: "var(--bg-card)",
                color: "var(--text-primary)",
              }}
            >
              <option value="">All Channels</option>
              <For each={channels()}>
                {(ch) => (
                  <option value={ch.id}>
                    {ch.channelNumber} - {ch.name}
                  </option>
                )}
              </For>
            </select>
          </div>

          <div style={{ display: "grid", "grid-template-columns": "1fr 1fr", gap: "12px" }}>
            <div>
              <label
                for="newRulePrePaddingInput"
                style={{
                  "font-size": "11px",
                  color: "var(--text-muted)",
                  display: "block",
                  "margin-bottom": "4px",
                }}
              >
                Pre-padding (mins)
              </label>
              <input
                id="newRulePrePaddingInput"
                type="number"
                value={newRulePrePadding()}
                onInput={(e) =>
                  setNewRulePrePadding(Number.parseInt(e.currentTarget.value, 10) || 0)
                }
                style={{
                  width: "100%",
                  padding: "6px",
                  "border-radius": "var(--radius-sm)",
                  border: "1px solid var(--border)",
                  background: "var(--bg-card)",
                  color: "var(--text-primary)",
                }}
              />
            </div>
            <div>
              <label
                for="newRulePostPaddingInput"
                style={{
                  "font-size": "11px",
                  color: "var(--text-muted)",
                  display: "block",
                  "margin-bottom": "4px",
                }}
              >
                Post-padding (mins)
              </label>
              <input
                id="newRulePostPaddingInput"
                type="number"
                value={newRulePostPadding()}
                onInput={(e) =>
                  setNewRulePostPadding(Number.parseInt(e.currentTarget.value, 10) || 0)
                }
                style={{
                  width: "100%",
                  padding: "6px",
                  "border-radius": "var(--radius-sm)",
                  border: "1px solid var(--border)",
                  background: "var(--bg-card)",
                  color: "var(--text-primary)",
                }}
              />
            </div>
          </div>

          <div
            style={{ display: "flex", "align-items": "center", gap: "8px", "margin-top": "4px" }}
          >
            <input
              type="checkbox"
              id="onlyNewCheckbox"
              checked={newRuleOnlyNew()}
              onChange={(e) => setNewRuleOnlyNew(e.currentTarget.checked)}
            />
            <label for="onlyNewCheckbox" style={{ "font-size": "13px" }}>
              Only record new episodes (ignore re-runs)
            </label>
          </div>

          <div style={{ display: "flex", "flex-direction": "column", gap: "10px" }}>
            <label>
              <input
                type="checkbox"
                checked={newRuleSeriesOnly()}
                onChange={(e) => setNewRuleSeriesOnly(e.currentTarget.checked)}
              />{" "}
              Only episodic programmes
            </label>
            <label for="newRuleSeason">Season (leave empty for all)</label>
            <input
              id="newRuleSeason"
              type="number"
              min="0"
              max="10000"
              value={newRuleSeason()}
              onInput={(e) => setNewRuleSeason(e.currentTarget.value)}
            />
            <label for="newRuleKeepMax">
              Keep latest saved recordings (leave empty to keep all)
            </label>
            <input
              id="newRuleKeepMax"
              type="number"
              min="1"
              max="10000"
              value={newRuleKeepMax()}
              onInput={(e) => setNewRuleKeepMax(e.currentTarget.value)}
            />
            <label>
              <input
                type="checkbox"
                checked={newRuleDeleteWatched()}
                onChange={(e) => setNewRuleDeleteWatched(e.currentTarget.checked)}
              />{" "}
              Delete after my profile watches
            </label>
            <p style={{ color: "var(--text-secondary)", "font-size": "12px" }}>
              Retention automatically deletes saved files for this rule. Active playback is kept.
              Profiles with an age limit record only the guide programme window, without extra
              padding.
            </p>
          </div>

          <div
            style={{
              display: "flex",
              "justify-content": "flex-end",
              gap: "8px",
              "margin-top": "8px",
            }}
          >
            <Button variant="secondary" onClick={() => setRuleModalOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" onClick={handleCreateRuleSubmit}>
              Create Series Rule
            </Button>
          </div>
        </div>
      </Dialog>

      {/* =======================================================================
          MODAL: CREATE VIRTUAL CHANNEL
          ======================================================================= */}
      <Dialog
        open={createVcModalOpen()}
        onOpenChange={setCreateVcModalOpen}
        title="Create Virtual 24/7 Channel"
        description="Create a synthetic continuous linear broadcast channel from library items."
      >
        <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
          <div>
            <label
              for="newVcNameInput"
              style={{
                "font-size": "12px",
                "font-weight": "600",
                display: "block",
                "margin-bottom": "4px",
              }}
            >
              Channel Name
            </label>
            <input
              id="newVcNameInput"
              type="text"
              placeholder="e.g. Comedy 24/7, Sci-Fi Classics"
              value={newVcName()}
              onInput={(e) => setNewVcName(e.currentTarget.value)}
              style={{
                width: "100%",
                padding: "8px 10px",
                "border-radius": "var(--radius-sm)",
                border: "1px solid var(--border)",
                background: "var(--bg-card)",
                color: "var(--text-primary)",
              }}
            />
          </div>

          <div>
            <label
              for="newVcNumberInput"
              style={{
                "font-size": "12px",
                "font-weight": "600",
                display: "block",
                "margin-bottom": "4px",
              }}
            >
              Channel Number
            </label>
            <input
              id="newVcNumberInput"
              type="text"
              value={newVcNumber()}
              onInput={(e) => setNewVcNumber(e.currentTarget.value)}
              style={{
                width: "100%",
                padding: "8px 10px",
                "border-radius": "var(--radius-sm)",
                border: "1px solid var(--border)",
                background: "var(--bg-card)",
                color: "var(--text-primary)",
              }}
            />
          </div>

          <div>
            <label
              for="newVcModeSelect"
              style={{
                "font-size": "12px",
                "font-weight": "600",
                display: "block",
                "margin-bottom": "4px",
              }}
            >
              Playback Mode
            </label>
            <select
              id="newVcModeSelect"
              value={newVcMode()}
              onChange={(e) => setNewVcMode(e.currentTarget.value as "linear_loop" | "shuffle")}
              style={{
                width: "100%",
                padding: "8px 10px",
                "border-radius": "var(--radius-sm)",
                border: "1px solid var(--border)",
                background: "var(--bg-card)",
                color: "var(--text-primary)",
              }}
            >
              <option value="linear_loop">Linear Loop (Sequential in order)</option>
              <option value="shuffle">Deterministic Shuffle</option>
            </select>
          </div>

          <div
            style={{
              display: "flex",
              "justify-content": "flex-end",
              gap: "8px",
              "margin-top": "8px",
            }}
          >
            <Button variant="secondary" onClick={() => setCreateVcModalOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" onClick={handleCreateVirtualChannel}>
              Create Channel
            </Button>
          </div>
        </div>
      </Dialog>

      {/* =======================================================================
          MODAL: BROWSE PLUGIN FEEDS
          ======================================================================= */}
      <Dialog
        open={pluginModalOpen()}
        onOpenChange={setPluginModalOpen}
        title={`${selectedPlugin()?.name || "Plugin"} Channels & Feeds`}
        description="Select a feed to browse online streaming channels and media items."
      >
        <div style={{ display: "flex", "flex-direction": "column", gap: "16px" }}>
          {/* Feed Tabs / Pills */}
          <div style={{ display: "flex", "flex-wrap": "wrap", gap: "6px" }}>
            <For each={pluginFeeds()}>
              {(feed) => (
                <Button
                  variant={activeFeedTitle() === feed.title ? "primary" : "secondary"}
                  onClick={() => handleSelectFeed(selectedPlugin()!.id, feed.id, feed.title)}
                >
                  {feed.title}
                </Button>
              )}
            </For>
          </div>

          {/* Active Feed Items */}
          <div
            style={{
              "max-height": "360px",
              "overflow-y": "auto",
              display: "flex",
              "flex-direction": "column",
              gap: "8px",
            }}
          >
            <Show
              when={activeFeedItems().length > 0}
              fallback={<div style={{ color: "var(--text-muted)" }}>No items in this feed.</div>}
            >
              <For each={activeFeedItems()}>
                {(item) => (
                  <div
                    style={{
                      display: "flex",
                      "align-items": "center",
                      "justify-content": "space-between",
                      padding: "8px 12px",
                      background: "var(--bg-base)",
                      "border-radius": "var(--radius-sm)",
                      border: "1px solid var(--border)",
                    }}
                  >
                    <div>
                      <div style={{ "font-weight": "700", "font-size": "13px" }}>{item.title}</div>
                      {item.durationMs && (
                        <div style={{ "font-size": "11px", color: "var(--text-muted)" }}>
                          {Math.floor(item.durationMs / 60000)} mins
                        </div>
                      )}
                    </div>
                    <Button
                      variant="primary"
                      onClick={() => {
                        showToast({
                          title: "Streaming Media",
                          message: `Playing "${item.title}" via ${selectedPlugin()?.name}`,
                          type: "info",
                        });
                      }}
                    >
                      ▶ Play
                    </Button>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </div>
      </Dialog>
    </div>
  );
}

export default LiveTv;
