import { createContext, createSignal, type JSX, lazy, Show, useContext } from "solid-js";

const Player = lazy(() => import("../components/Player"));

export interface PlayRequest {
  autoplayNext?: boolean;
  itemId: number;
  partId?: number;
  initialStartMs?: number;
  audioStreamIndex?: number;
  subtitleStreamIndex?: number | null;
  title?: string;
  subtitle?: string;
  /** Quality limit in bit/s chosen before playback. */
  maxBitrate?: number;
  /** Open the Cast chooser first instead of starting a local stream. */
  castFirst?: boolean;
}

interface PlayerContextType {
  playItem: (req: PlayRequest) => void;
  /** Plays `items` in order, starting at `start`, and continues through the list. */
  playQueue: (items: PlayRequest[], start?: number) => void;
  /** Adds to the end of the queue, or starts playing when nothing plays. */
  addToQueue: (req: PlayRequest) => void;
  queue: () => PlayRequest[];
  queueIndex: () => number;
  closePlayer: () => void;
  activeItem: () => PlayRequest | null;
  playbackRevision: () => number;
}

const PlayerContext = createContext<PlayerContextType>();

export function PlayerProvider(props: { children: JSX.Element }) {
  const [playbackRevision, setPlaybackRevision] = createSignal(0);
  const [activeItem, setActiveItem] = createSignal<PlayRequest | null>(null);

  const [queue, setQueue] = createSignal<PlayRequest[]>([]);
  const [queueIndex, setQueueIndex] = createSignal(0);

  // Playing a single title replaces any queue.
  const playItem = (req: PlayRequest) => {
    setQueue([]);
    setQueueIndex(0);
    setActiveItem(req);
  };

  const playQueue = (items: PlayRequest[], start = 0) => {
    if (items.length === 0) return;
    const index = Math.max(0, Math.min(items.length - 1, start));
    setQueue(items);
    setQueueIndex(index);
    setActiveItem({ ...items[index] });
  };

  const addToQueue = (req: PlayRequest) => {
    if (!activeItem()) {
      playQueue([req]);
      return;
    }
    if (queue().length === 0) setQueue([activeItem() as PlayRequest]);
    setQueue((list) => [...list, req]);
  };

  const step = (delta: number) => {
    const index = queueIndex() + delta;
    const next = queue()[index];
    if (!next) return false;
    setQueueIndex(index);
    setActiveItem({ ...next });
    return true;
  };

  const closePlayer = () => {
    setQueue([]);
    setQueueIndex(0);
    setActiveItem(null);
  };

  return (
    <PlayerContext.Provider
      value={{
        playItem,
        playQueue,
        addToQueue,
        queue,
        queueIndex,
        closePlayer,
        activeItem,
        playbackRevision,
      }}
    >
      {props.children}
      <Show keyed when={activeItem()}>
        {(item) => (
          <Player
            itemId={item.itemId}
            partId={item.partId}
            initialStartMs={item.initialStartMs}
            audioStreamIndex={item.audioStreamIndex}
            subtitleStreamIndex={item.subtitleStreamIndex}
            title={item.title}
            subtitle={item.subtitle}
            onClose={closePlayer}
            onStopped={() => setPlaybackRevision((v) => v + 1)}
            onPlayNext={playItem}
            autoplayNext={item.autoplayNext}
            maxBitrate={item.maxBitrate}
            castFirst={item.castFirst}
            queueNext={() => queue()[queueIndex() + 1] ?? null}
            queuePrevious={() => queue()[queueIndex() - 1] ?? null}
            queuePosition={() =>
              queue().length > 1 ? `${queueIndex() + 1} / ${queue().length}` : null
            }
            onQueueStep={step}
          />
        )}
      </Show>
    </PlayerContext.Provider>
  );
}

export function usePlayer() {
  const context = useContext(PlayerContext);
  if (!context) {
    throw new Error("usePlayer must be used within a PlayerProvider");
  }
  return context;
}
